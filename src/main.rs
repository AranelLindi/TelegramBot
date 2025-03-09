use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use tokio::sync::Mutex;
use std::{collections::HashMap, sync::Arc};
use simplelog::*;
use std::fs::File;
use dotenv::dotenv;
use std::env;
use log::info;
use reqwest;
use serde::{Serialize, Deserialize};

// Struktur für JSON Daten
#[derive(Serialize, Deserialize, Debug)]
struct SensorData {
    temperature: f64,
    humidity: f64,
}

// Benutzerkonfiguration
#[derive(Default)]
struct UserConfig {
    threshold_temp: Option<f64>,
    threshold_humidity: Option<f64>,
}

type UserConfigs = Arc<Mutex<HashMap<i64, UserConfig>>>;

// Telegram-Befehle
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Verfügbare Befehle:")]
enum Command {
    #[command(description = "Startet den Bot.")]
    Start,
    #[command(description = "Zeigt die Befehle an.")]
    Help,
    #[command(description = "Setzt den Temperaturschwellwert. Beispiel: /settemp 25.5")]
    SetTemp(f64),
    #[command(description = "Setzt den Feuchtigkeitsschwellwert. Beispiel: /sethumidity 60.0")]
    SetHumidity(f64),
    #[command(description = "Zeigt die aktuellen Sensordaten an.")]
    Status,
}

// Sensordaten von Webserver abrufen
async fn fetch_sensor_data() -> Option<SensorData> {
    println!("🛠 DEBUG: Starte HTTP-Anfrage an localhost:8080/sensors");

    let response = reqwest::get("http://localhost:8080/sensors").await;

    match response {
        Ok(mut resp) => {
            println!("✅ HTTP-Anfrage erfolgreich! Status: {}", resp.status());

            match resp.bytes().await {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    println!("📜 Erhaltener JSON-Text: {}", text);

                    match serde_json::from_str::<SensorData>(&text) {
                        Ok(data) => {
                            println!("📊 Erhaltene Sensordaten: Temp: {:.2}°C, Feuchte: {:.2}%", data.temperature, data.humidity);
                            Some(data)
                        }
                        Err(err) => {
                            println!("❌ Fehler beim JSON-Parsing: {:?}\n🔎 JSON-Text: {}", err, text);
                            None
                        }
                    }
                }
                Err(err) => {
                    println!("❌ Fehler beim Abrufen der Antwort als Bytes: {:?}", err);
                    None
                }
            }
        }
        Err(err) => {
            println!("❌ Fehler bei der HTTP-Anfrage: {:?}", err);
            None
        }
    }
}


#[tokio::main]
async fn main() {
    dotenv().ok();

    let token = env::var("TELEGRAMBOT_TOKEN")
        .expect("TELEGRAM_BOT_TOKEN nicht gesetzt!");

    CombinedLogger::init(vec![
        TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
        WriteLogger::new(LevelFilter::Info, Config::default(), File::create("bot.log").unwrap()),
    ]).unwrap();

    info!("Bot gestartet...");
    
    let bot = Bot::new(token);
    let user_configs: UserConfigs = Arc::new(Mutex::new(HashMap::new()));

    let cloned_bot = bot.clone();
    let cloned_configs = user_configs.clone();
    
    tokio::spawn(async move {
        println!("🚀 Sensordaten-Überwachung gestartet...");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await; // Kurzes Warten beim Start
    
        loop {
            match fetch_sensor_data().await {
                Some(sensor_data) => {
                    println!("✅ Sensordaten erhalten, überprüfe Schwellenwerte...");
                    
                    println!("🔒 Versuche, den Mutex zu sperren...");
                    let mut configs = cloned_configs.lock().await;
                    println!("🔓 Mutex erfolgreich gesperrt!");
    
                    if configs.is_empty() {
                        println!("⚠ Kein Benutzer hat Schwellwerte gesetzt!");
                    } else {
                        println!("👤 Anzahl der Benutzer mit Schwellwerten: {}", configs.len());
                    }
    
                    let mut users_to_remove = Vec::new(); // Nutzer merken, deren Schwellenwerte entfernt werden
    
                    for (&user_id, config) in configs.iter_mut() {
                        println!("👤 Prüfe User-ID: {}", user_id);
                        let mut warning_triggered = false;
    
                        if let Some(threshold) = config.threshold_temp {
                            println!("🌡 Temp-Schwelle: {:.2}°C", threshold);
                            if sensor_data.temperature > threshold {
                                println!("⚠ Temperaturwarnung für User {}: {:.2}°C", user_id, sensor_data.temperature);
                                let _ = cloned_bot.send_message(ChatId(user_id), 
                                    format!("⚠ Temperatur überschritten: {:.2}°C!\nℹ Der Schwellwert wurde zurückgesetzt. Stelle mit /settemp einen neuen Wert ein.", sensor_data.temperature))
                                    .await;
                                warning_triggered = true;
                            }
                        }
                        if let Some(threshold) = config.threshold_humidity {
                            println!("💧 Feuchte-Schwelle: {:.2}%", threshold);
                            if sensor_data.humidity > threshold {
                                println!("⚠ Feuchtigkeitswarnung für User {}: {:.2}%", user_id, sensor_data.humidity);
                                let _ = cloned_bot.send_message(ChatId(user_id), 
                                    format!("⚠ Feuchtigkeit überschritten: {:.2}%!\nℹ Der Schwellwert wurde zurückgesetzt. Stelle mit /sethumidity einen neuen Wert ein.", sensor_data.humidity))
                                    .await;
                                warning_triggered = true;
                            }
                        }
    
                        if warning_triggered {
                            users_to_remove.push(user_id); // Nutzer zum Entfernen vormerken
                        }
                    }
    
                    // Entferne die Schwellenwerte der Nutzer, bei denen eine Warnung ausgelöst wurde
                    for user_id in users_to_remove {
                        if let Some(config) = configs.get_mut(&user_id) {
                            config.threshold_temp = None;
                            config.threshold_humidity = None;
                        }
                    }
                }
                None => {
                    println!("❌ Konnte Sensordaten nicht abrufen. Warte 60 Sekunden und versuche erneut...");
                }
            }
    
            // 60 Sekunden warten, bevor erneut geprüft wird
            println!("⏳ Warte 60 Sekunden...");
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    });
    
    

    // Dispatcher erstellen
    let handler = Update::filter_message()
        .branch(
            // 👇 Hier wird `Command::parse()` korrekt genutzt!
            dptree::entry()
                .filter_command::<Command>()
                .endpoint(answer),
        )
        .branch(dptree::endpoint(handle_message)); // Freie Texte verarbeiten

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![user_configs])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

async fn answer(bot: Bot, msg: Message, cmd: Command, configs: UserConfigs) -> ResponseResult<()> {
    let user_id = msg.chat.id;
    let mut user_configs = configs.lock().await;

    match cmd {
        Command::Start => {
            bot.send_message(user_id, "👋 Willkommen! Nutze /settemp oder /sethumidity, um Schwellwerte festzulegen.").await?;
        }
        Command::SetTemp(value) => {
            user_configs.entry(user_id.0).or_default().threshold_temp = Some(value);
            bot.send_message(user_id, format!("✅ Schwellwert für Temperatur gesetzt: {:.2}°C", value)).await?;
        }
        Command::SetHumidity(value) => {
            user_configs.entry(user_id.0).or_default().threshold_humidity = Some(value);
            bot.send_message(user_id, format!("✅ Schwellwert für Feuchtigkeit gesetzt: {:.2}%", value)).await?;
        }
        Command::Status => {
            bot.send_message(user_id, "📊 Aktuelle Sensordaten: 🌡 Temperatur: 22.5°C 💧 Feuchtigkeit: 45.0%").await?;
        }
        Command::Help => {
            bot.send_message(user_id, "Quatschkopf!").await?;
        }
    }
    Ok(())
}

async fn handle_message(bot: Bot, msg: Message) -> ResponseResult<()> {
    if let Some(text) = msg.text() {
        let user_id = msg.chat.id;
        let response = match text.to_lowercase().as_str() {
            "hallo" => "👋 Hallo Julia! Wie kann ich helfen?",
            "wie geht's?" => "Mir geht es super! 🤖",
            "ich liebe dich" => "Ich liebe dich auch",
            _ => "Ich habe dich nicht verstanden. Nutze /help für Befehle.",
        }; // alles in Kleinschreibung angeben!!
        bot.send_message(user_id, response).await?;
    }
    Ok(())
}
