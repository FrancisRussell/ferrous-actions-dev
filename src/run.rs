use crate::cache_cargo_home::{restore_cargo_cache, save_cargo_cache};
use crate::cross::Cross;
use crate::input_manager::{Input, Manager as InputManager};
use crate::rustup::{self, ToolchainConfig};
use crate::{node, toolchain, warning, Cargo, Error};
use alloc::string::String;
use alloc::vec::Vec;

fn get_toolchain_config(input_manager: &InputManager) -> Result<ToolchainConfig, Error> {
    let mut toolchain_config = ToolchainConfig::default();
    if let Some(toolchain) = input_manager.get(Input::Toolchain) {
        toolchain_config.name = toolchain.into();
    }
    if let Some(profile) = input_manager.get(Input::Profile) {
        toolchain_config.profile = profile.into();
    }
    if let Some(components) = input_manager.get(Input::Components) {
        toolchain_config.components = components.split_whitespace().map(String::from).collect();
    }
    if let Some(targets) = input_manager.get(Input::Targets) {
        toolchain_config.targets = targets.split_whitespace().map(String::from).collect();
    }
    if let Some(set_default) = input_manager.get(Input::Default) {
        let set_default = set_default
            .parse::<bool>()
            .map_err(|_| Error::OptionParseError(Input::Default.to_string(), set_default.to_string()))?;
        toolchain_config.set_default = set_default;
    }
    if let Some(set_override) = input_manager.get(Input::Override) {
        let set_override = set_override
            .parse::<bool>()
            .map_err(|_| Error::OptionParseError(Input::Override.to_string(), set_override.to_string()))?;
        toolchain_config.set_override = set_override;
    }
    Ok(toolchain_config)
}

pub async fn run() -> Result<(), Error> {
    use wasm_bindgen::JsError;

    let environment = node::process::get_env();
    if let Some(phase) = environment.get("GITHUB_RUST_ACTION_PHASE") {
        match phase.as_str() {
            "main" => main().await,
            "post" => post().await,
            _ => {
                warning!("Unexpectedly invoked with phase {}. Doing nothing.", phase);
                Ok(())
            }
        }
    } else {
        Err(Error::Js(
            JsError::new("Action was invoked in an unexpected way. Could not determine phase.").into(),
        ))
    }
}

pub async fn main() -> Result<(), Error> {
    let input_manager = InputManager::build()?;
    let command = input_manager.get_required(Input::Command)?;
    let split: Vec<&str> = command.split_whitespace().collect();
    match split[..] {
        ["install-rustup"] => {
            let toolchain_config = get_toolchain_config(&input_manager)?;
            rustup::install(&toolchain_config).await?;
        }
        ["install-toolchain"] => {
            let toolchain_config = get_toolchain_config(&input_manager)?;
            toolchain::install(&toolchain_config).await?;
        }
        ["cargo", cargo_subcommand] => {
            let use_cross = if let Some(use_cross) = input_manager.get(Input::UseCross) {
                use_cross
                    .parse::<bool>()
                    .map_err(|_| Error::OptionParseError("use-cross".into(), use_cross.to_string()))?
            } else {
                false
            };
            let mut cargo = if use_cross {
                let cross = Cross::get_or_install(&input_manager).await?;
                Cargo::from_path(&cross.get_path()).await?
            } else {
                Cargo::from_environment().await?
            };
            let cargo_args = input_manager.get(Input::Args).unwrap_or_default();
            let cargo_args =
                shlex::split(cargo_args).ok_or_else(|| Error::ArgumentsParseError(cargo_args.to_string()))?;
            let toolchain = input_manager.get(Input::Toolchain);
            cargo
                .run(
                    toolchain,
                    cargo_subcommand,
                    cargo_args.iter().map(String::as_str),
                    &input_manager,
                )
                .await?;
        }
        ["cache"] => restore_cargo_cache(&input_manager).await?,
        _ => return Err(Error::UnknownCommand(command.to_string())),
    }

    for input in input_manager.unused() {
        warning!("Recognised but unused input {} was passed to action", input);
    }
    Ok(())
}

pub async fn post() -> Result<(), Error> {
    let input_manager = InputManager::build()?;
    let command = input_manager.get_required(Input::Command)?;
    let split: Vec<&str> = command.split_whitespace().collect();
    #[allow(clippy::single_match)]
    match split[..] {
        ["cache"] => save_cargo_cache(&input_manager).await?,
        _ => {}
    }
    Ok(())
}
