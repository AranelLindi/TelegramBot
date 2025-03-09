use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use tokio::sync::Mutex;
use std::{collections::HashMap, sync::Arc};
use simplelog::*;
use std::fs::File;
use dotenv::dotenv;
use std::env;
use log::info;

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
        };
        bot.send_message(user_id, response).await?;
    }
    Ok(())
}
