mod server;

use crate::server::start_server;
use log::info;
use std::env;
use std::sync::Arc;
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::{
    dispatching::{dialogue, UpdateHandler},
    prelude::*,
    utils::command::BotCommands,
};
use tokio::sync::Mutex;

type MyDialogue = Dialogue<State, InMemStorage<State>>;
type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Default)]
pub enum State {
    #[default]
    Initial,
    ReceiveTemp,
}

/// These commands are supported:
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
    Help,
    Status,
    SetTemp,
    On,
    Off,
    Cancel,
}

pub struct HeatingState {
    pub target_temp: f64,
    pub current_temp: f64,
    pub heating_is_on: bool,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Starting Heating Control bot...");

    let bot = Bot::from_env();

    let webhook_port = env::var("PORT").unwrap();

    let authorized_users: Vec<i64> = env::var("AUTHORIZED_USER_IDS")
        .unwrap()
        .split(' ')
        .map(|s| s.parse::<i64>().unwrap())
        .collect();

    let heating_state = Arc::new(Mutex::new(HeatingState {
        target_temp: 21.0,
        current_temp: 0.0,
        heating_is_on: false,
    }));

    tokio::spawn(start_server(webhook_port, heating_state.clone()));

    Dispatcher::builder(bot, schema())
        .dependencies(dptree::deps![
            InMemStorage::<State>::new(),
            heating_state,
            authorized_users
        ])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

fn schema() -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
    use dptree::case;

    let command_handler = teloxide::filter_command::<Command, _>()
        .branch(
            case![State::Initial]
                .branch(case![Command::Help].endpoint(help))
                .branch(case![Command::Status].endpoint(status))
                .branch(case![Command::SetTemp].endpoint(set_temp))
                .branch(case![Command::On].endpoint(set_heating_on))
                .branch(case![Command::Off].endpoint(set_heating_off)),
        )
        .branch(case![Command::Cancel].endpoint(cancel));

    let message_handler = Update::filter_message()
        .chain(dptree::filter_async(check_valid_user))
        .branch(command_handler)
        .branch(case![State::ReceiveTemp].endpoint(receive_temp))
        .branch(dptree::endpoint(invalid_state));

    dialogue::enter::<Update, InMemStorage<State>, State, _>().branch(message_handler)
}

async fn help(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}

async fn cancel(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, "Cancelling the dialogue.")
        .await?;
    dialogue.exit().await?;
    Ok(())
}

async fn invalid_state(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(
        msg.chat.id,
        "Unable to handle the message. Type /help to see the usage.",
    )
    .await?;
    Ok(())
}

async fn status(bot: Bot, msg: Message, heating_state: Arc<Mutex<HeatingState>>) -> HandlerResult {
    let state = heating_state.lock().await;

    let on_off = match state.heating_is_on {
        true => "ON",
        false => "OFF",
    };
    bot.send_message(
        msg.chat.id,
        format!(
            "\
        Current status: \n\
         * Heating is {on_off} \n\
         * Target temperature is {} \n\
         * Current temperature is {}",
            state.target_temp, state.current_temp,
        ),
    )
    .await?;

    Ok(())
}

async fn set_temp(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, "Enter the target temperature to set up")
        .await?;
    dialogue.update(State::ReceiveTemp).await?;
    Ok(())
}

async fn receive_temp(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    heating_state: Arc<Mutex<HeatingState>>,
) -> HandlerResult {
    match msg.text().map(ToOwned::to_owned) {
        Some(temperature) => {
            let temperature = match temperature.parse::<f64>() {
                Ok(t) => t,
                Err(_) => {
                    bot.send_message(
                        dialogue.chat_id(),
                        "That's an invalid target temperature. Please provide valid temperature."
                            .to_string(),
                    )
                    .await?;

                    return Ok(());
                }
            };

            heating_state.lock().await.target_temp = temperature;

            bot.send_message(
                msg.chat.id,
                format!("Target temperature set to {temperature}"),
            )
            .await?;

            dialogue.exit().await?;
        }
        None => {
            bot.send_message(
                msg.chat.id,
                "Please, send me the target temperature you want to set up",
            )
            .await?;
        }
    }
    Ok(())
}

async fn set_heating_on(
    bot: Bot,
    msg: Message,
    heating_state: Arc<Mutex<HeatingState>>,
) -> HandlerResult {
    heating_state.lock().await.heating_is_on = true;

    bot.send_message(msg.chat.id, "Heating set to ON").await?;
    Ok(())
}

async fn set_heating_off(
    bot: Bot,
    msg: Message,
    heating_state: Arc<Mutex<HeatingState>>,
) -> HandlerResult {
    heating_state.lock().await.heating_is_on = false;

    bot.send_message(msg.chat.id, "Heating set to OFF").await?;
    Ok(())
}

async fn check_valid_user(bot: Bot, msg: Message, authorized_users: Vec<i64>) -> bool {
    match authorized_users.contains(&msg.chat.id.0) {
        true => true,
        false => {
            let _ = bot.send_message(msg.chat.id, "Unauthorized user").await;
            false
        }
    }
}
