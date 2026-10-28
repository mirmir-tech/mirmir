use std::{
    fs,
    io::{self, Read},
};

pub mod benchmark;

use crate::{
    cli::PromptArgs,
    daemon::Connection,
    error::{Error, Result},
    instance::Instance,
    rpc::proto,
};

pub async fn run(instance: &Instance, args: PromptArgs) -> Result<()> {
    let prompt = read_prompt(&args)?;
    if args.samples == 0 {
        return Err(Error::Config("--samples must be positive".to_owned()));
    }
    let connection = instance.connect().await?;
    run_connection(connection, args, prompt).await
}

async fn run_connection(
    mut connection: Connection,
    args: PromptArgs,
    prompt: String,
) -> Result<()> {
    let reused = connection.reused();
    let result = async {
        let model = if let Some(model) = args.model.clone() {
            model
        } else {
            #[derive(serde::Deserialize)]
            struct Defaults {
                default_model: Option<String>,
            }
            let snapshot = connection
                .client
                .get_configuration(proto::GetConfigurationRequest {})
                .await?
                .into_inner();
            let defaults: Defaults = toml::from_str(&snapshot.raw_toml)?;
            defaults.default_model.ok_or_else(|| {
                Error::Config("set --model or default_model on the selected server".to_owned())
            })?
        };
        benchmark::execute(&mut connection.client, &args, model, prompt, reused).await
    }
    .await;
    result.and(connection.shutdown().await)
}

fn read_prompt(args: &PromptArgs) -> Result<String> {
    let value = if let Some(prompt) = &args.prompt {
        prompt.clone()
    } else if let Some(path) = &args.prompt_file {
        fs::read_to_string(path)?
    } else if args.stdin {
        let mut value = String::new();
        io::stdin().lock().read_to_string(&mut value)?;
        value
    } else {
        return Err(Error::Config(
            "provide exactly one of --prompt, --prompt-file, or --stdin".to_owned(),
        ));
    };
    if value.trim().is_empty() {
        Err(Error::Config("prompt cannot be empty".to_owned()))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::{
        config::{AppConfig, Paths},
        daemon,
    };

    static NEXT_PROMPT: AtomicU64 = AtomicU64::new(0);

    #[tokio::test]
    async fn zero_samples_does_not_start_server() {
        let (paths, mut args) = fixture();
        args.samples = 0;
        assert!(run(&Instance::Socket(paths.socket_file.clone()), args).await.is_err());
        assert!(!paths.socket_file.exists());
    }

    #[tokio::test]
    async fn ephemeral_server_is_cleaned_after_request_error() {
        let (paths, args) = fixture();
        let connection = daemon::connect_or_start(&paths, &AppConfig::default())
            .await
            .expect("ephemeral server");
        assert!(run_connection(connection, args, "hello".to_owned()).await.is_err());
        assert!(!paths.socket_file.exists());
    }

    fn fixture() -> (Paths, PromptArgs) {
        let id = NEXT_PROMPT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("mirmir-prompt-{}-{id}", std::process::id()));
        let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
        let args = PromptArgs {
            model: Some("missing-model".to_owned()),
            prompt: Some("hello".to_owned()),
            prompt_file: None,
            stdin: false,
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            repetition_penalty: None,
            seed: None,
            json: false,
            csv: None,
            no_stream: false,
            warmup: 0,
            samples: 1,
        };
        (paths, args)
    }
}
