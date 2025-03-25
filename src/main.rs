use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use simplelog::*;
use std::fs::File;
use dotenv::dotenv;
use std::env;
use log::info;
use reqwest;
use serde::{Serialize, Deserialize};
use teloxide::types::ParseMode;
use chrono::{NaiveDateTime, Local, TimeZone};

// Iteration in der neue Sensordaten abgerufen werden:
const ITERATION_IN_SECONDS: u64 = 10 * 60; // 10 minutes
// Wird das /status Kommando benutzt, wird nochmal extra abgefragt.
// Die ITERATION ist nur für Grenzwerte interessant und da reichen
// 10 Minuten.



// Struktur für JSON Daten
#[derive(Serialize, Deserialize, Debug, Clone)]
struct SensorData {
    device_id: String,   // Unique identifier for each sensor
    sensor_type: String, // Example: "temperature" or "humidity"
    value: f64,          // The measured value
    timestamp: i64,   // (Optional) If time tracking is wanted
}

// Benutzerkonfiguration
#[derive(Default)]
struct UserConfig {
    thresholds: HashMap<(String, String), f64>, // (sensor_id, sensor_type) -> threshold
}

type UserConfigs = Arc<Mutex<HashMap<i64, UserConfig>>>;


// Telegram-Befehle
#[derive(BotCommands, Clone)]
#[command(rename_rule = "kebab-case", description = "Verfügbare Befehle:")]
enum Command {
    #[command(description = "Startet den Bot.")]
    Start,
    #[command(description = "Zeigt diese Hilfe an.")]
    Help,
    #[command(description = "Zeigt alle aktuellen Sensordaten.")]
    Status,
    #[command(description = "Temperaturverlauf Wohnzimmer.")]
    WohnzimmerTdia,
    #[command(description = "Luftfeuchtigkeitsverlauf Wohnzimmer.")]
    WohnzimmerHdia,
    #[command(description = "Alarm, wenn Temperatur unter Wert fällt.")]
    WohnzimmerTmin(f64),
    #[command(description = "Alarm, wenn Temperatur über Wert steigt.")]
    WohnzimmerTmax(f64),
    #[command(description = "Alarm, wenn Luftfeuchtigkeit unter Wert fällt.")]
    WohnzimmerHmin(f64),
    #[command(description = "Alarm, wenn Luftfeuchtigkeit über Wert steigt.")]
    WohnzimmerHmax(f64),
}

// Sensordaten von Webserver abrufen
async fn fetch_sensor_data() -> Option<Vec<SensorData>> {
    println!("DEBUG: Starte HTTP-Anfrage an localhost:8080/sensors");

    let response = reqwest::get("http://localhost:8080/sensors").await;

    match response {
        Ok(resp) => match resp.text().await {
            Ok(text) => {
                match serde_json::from_str::<Vec<SensorData>>(&text) {
                    Ok(data) => {
                        //println!("📊 Erhaltene Sensordaten: {:?}", data);
                        Some(data) // Return multiple sensors
                    }
                    Err(err) => {
                        println!("Fehler beim JSON-Parsing: {:?}\nJSON-Text: {}", err, text);
                        None
                    }
                }
            }
            Err(err) => {
                println!("Fehler beim Abrufen der Antwort als Text: {:?}", err);
                None
            }
        },
        Err(err) => {
            println!("Fehler bei der HTTP-Anfrage: {:?}", err);
            None
        }
    }
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let token = env::var("TELEGRAMBOT_TOKEN").expect("TELEGRAMBOT_TOKEN nicht gesetzt!");

    CombinedLogger::init(vec![
        TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
        WriteLogger::new(LevelFilter::Info, Config::default(), File::create("bot.log").unwrap()),
    ]).unwrap();

    let bot = Bot::new(token);
    let user_configs: UserConfigs = Arc::new(Mutex::new(HashMap::new()));
    let threshold_flags: Arc<Mutex<HashMap<(i64, String, String), bool>>> = Arc::new(Mutex::new(HashMap::new()));

    // Sensor-Überwachung starten
    let bot_clone = bot.clone();
    let configs_clone = user_configs.clone();
    let flags_clone = threshold_flags.clone();

    tokio::spawn(async move {
        loop {
            if let Some(sensor_data_list) = fetch_sensor_data().await {
                let mut configs = configs_clone.lock().await;
                let mut flags = flags_clone.lock().await;

                for sensor in sensor_data_list {
                    for (&user_id, config) in configs.iter() {
                        let key_min = (sensor.device_id.clone(), format!("{}_min", sensor.sensor_type));
                        let key_max = (sensor.device_id.clone(), format!("{}_max", sensor.sensor_type));

                        let user_key_min = (user_id, sensor.device_id.clone(), key_min.1.clone());
                        let user_key_max = (user_id, sensor.device_id.clone(), key_max.1.clone());

                        if let Some(&min_val) = config.thresholds.get(&key_min) {
                            if sensor.value < min_val {
                                if flags.get(&user_key_min) != Some(&true) {
                                    let _ = bot_clone.send_message(ChatId(user_id), format!(
                                        "⚠ {} im {} ist unter die Schwelle gefallen: {:.1} (Schwelle: {:.1})",
                                        sensor.sensor_type, sensor.device_id, sensor.value, min_val
                                    )).await;
                                    flags.insert(user_key_min, true);
                                }
                            } else {
                                flags.insert(user_key_min, false);
                            }
                        }

                        if let Some(&max_val) = config.thresholds.get(&key_max) {
                            if sensor.value > max_val {
                                if flags.get(&user_key_max) != Some(&true) {
                                    let _ = bot_clone.send_message(ChatId(user_id), format!(
                                        "⚠ {} im {} ist über die Schwelle gestiegen: {:.1} (Schwelle: {:.1})",
                                        sensor.sensor_type, sensor.device_id, sensor.value, max_val
                                    )).await;
                                    flags.insert(user_key_max, true);
                                }
                            } else {
                                flags.insert(user_key_max, false);
                            }
                        }
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(ITERATION_IN_SECONDS)).await;
        }
    });

    // Dispatcher starten
    let handler = Update::filter_message()
        .branch(dptree::entry().filter_command::<Command>().endpoint(answer));

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![user_configs, threshold_flags])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

async fn answer(
    bot: Bot,
    msg: Message,
    cmd: Command,
    configs: UserConfigs,
    // threshold_flags ist hier nicht nötig
) -> ResponseResult<()> {
    let user_id = msg.chat.id;
    let mut user_configs = configs.lock().await;

    match cmd {
        Command::Start => {
            bot.send_message(user_id, "👋 Willkommen! Nutze /help für alle Befehle.").await?;
        }

        Command::Help => {
            let text = Command::descriptions();
            bot.send_message(user_id, format!("📖 *Hilfe:*\n{}", text))
                .parse_mode(ParseMode::Markdown)
                .await?;
        }

        Command::Status => {
            if let Some(sensor_data) = fetch_sensor_data().await {
                let mut text = String::from("📊 *Aktuelle Sensordaten:*\n");

                for entry in sensor_data {
                    let raum = match entry.device_id.as_str() {
                        "sensor1" => "Wohnzimmer",
                        _ => &entry.device_id,
                    };

                    let (typ, einheit) = match entry.sensor_type.as_str() {
                        "temperature" => ("Temperatur", "°C"),
                        "humidity" => ("Luftfeuchtigkeit", "%"),
                        _ => (&entry.sensor_type[..], ""),
                    };

                    let dt = NaiveDateTime::from_timestamp_opt(entry.timestamp as i64, 0)
                    .unwrap_or_else(|| NaiveDateTime::from_timestamp(0, 0));
                    let zeit = Local.from_utc_datetime(&dt);
                
                    let formatted = zeit.format("%d.%m.%Y %H:%M:%S");

                    text.push_str(&format!("📍 *{}* – {}: *{:.1} {}* ({})\n", raum, typ, entry.value, einheit, formatted));
                }

                bot.send_message(user_id, text)
                    .parse_mode(ParseMode::Markdown)
                    .await?;
            } else {
                bot.send_message(user_id, "❌ Fehler beim Abrufen der Sensordaten.").await?;
            }
        }

        Command::WohnzimmerTdia => {
            let url = "https://thingspeak.mathworks.com/channels/1115568/charts/1?...";
            bot.send_message(user_id, "📈 *Temperaturverlauf Wohnzimmer:*")
                .parse_mode(ParseMode::Markdown)
                .await?;
            bot.send_message(user_id, url).disable_web_page_preview(false).await?;
        }

        Command::WohnzimmerHdia => {
            let url = "https://thingspeak.mathworks.com/channels/1115568/charts/2?...";
            bot.send_message(user_id, "💧 *Luftfeuchtigkeit Wohnzimmer:*")
                .parse_mode(ParseMode::Markdown)
                .await?;
            bot.send_message(user_id, url).disable_web_page_preview(false).await?;
        }

        Command::WohnzimmerTmin(value) => {
            user_configs.entry(user_id.0).or_default()
                .thresholds.insert(("Wohnzimmer".into(), "temperature_min".into()), value);

            bot.send_message(user_id, format!("🔻 MIN-Schwellwert Temperatur Wohnzimmer: {:.1} °C", value)).await?;
        }

        Command::WohnzimmerTmax(value) => {
            user_configs.entry(user_id.0).or_default()
                .thresholds.insert(("Wohnzimmer".into(), "temperature_max".into()), value);

            bot.send_message(user_id, format!("🔺 MAX-Schwellwert Temperatur Wohnzimmer: {:.1} °C", value)).await?;
        }

        Command::WohnzimmerHmin(value) => {
            user_configs.entry(user_id.0).or_default()
                .thresholds.insert(("Wohnzimmer".into(), "humidity_min".into()), value);

            bot.send_message(user_id, format!("🔻 MIN-Schwellwert Luftfeuchtigkeit Wohnzimmer: {:.1} %", value)).await?;
        }

        Command::WohnzimmerHmax(value) => {
            user_configs.entry(user_id.0).or_default()
                .thresholds.insert(("Wohnzimmer".into(), "humidity_max".into()), value);

            bot.send_message(user_id, format!("🔺 MAX-Schwellwert Luftfeuchtigkeit Wohnzimmer: {:.1} %", value)).await?;
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
