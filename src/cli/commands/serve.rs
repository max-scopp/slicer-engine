//! Serve command – starts a local HTTP server to host the Angular UI.

use clap::Parser;

/// Serve the bundled Angular UI over a local HTTP server
#[derive(Parser, Debug)]
pub struct ServeCommand {
    /// Port to listen on
    #[arg(short, long, default_value_t = 4200)]
    pub port: u16,

    /// Directory containing the built Angular app
    /// (defaults to `./ui/dist/slicer-ui/browser`)
    #[arg(long, default_value = "./ui/dist/slicer-ui/browser")]
    pub ui_dir: String,

    /// Host address to bind
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
}

impl ServeCommand {
    /// Execute the serve command
    pub fn execute(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ui_dir = std::path::PathBuf::from(&self.ui_dir);

        if !ui_dir.exists() {
            return Err(format!(
                "UI directory not found: {}\n\
                 Build the Angular app first:\n\
                 \n  cd ui && npm run build\n",
                ui_dir.display()
            )
            .into());
        }

        let host = self.host.clone();
        let port = self.port;
        let ui_dir = self.ui_dir.clone();

        eprintln!("Serving Slicer Engine UI at http://{}:{}/", host, port);
        eprintln!("Serving files from: {}", ui_dir);
        eprintln!("Press Ctrl+C to stop.");

        tokio::runtime::Runtime::new()?.block_on(run_server(host, port, ui_dir))?;
        Ok(())
    }
}

async fn run_server(
    host: String,
    port: u16,
    ui_dir: String,
) -> Result<(), Box<dyn std::error::Error>> {
    use actix_files::Files;
    use actix_web::{web, App, HttpServer};

    HttpServer::new(move || {
        let fallback_dir = ui_dir.clone();
        App::new().service(
            // Serve static assets; fall back to index.html for SPA navigation
            Files::new("/", &ui_dir)
                .index_file("index.html")
                .default_handler(web::to(move || {
                    let path = format!("{}/index.html", fallback_dir);
                    async move {
                        actix_files::NamedFile::open_async(path)
                            .await
                            .map_err(actix_web::error::ErrorNotFound)
                    }
                })),
        )
    })
    .bind((host.as_str(), port))?
    .run()
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serve_command_defaults() {
        let cmd = ServeCommand {
            port: 4200,
            ui_dir: "./ui/dist/slicer-ui/browser".to_string(),
            host: "127.0.0.1".to_string(),
        };
        assert_eq!(cmd.port, 4200);
        assert_eq!(cmd.host, "127.0.0.1");
    }

    #[test]
    fn test_serve_command_missing_dir_error() {
        let cmd = ServeCommand {
            port: 4200,
            ui_dir: "/nonexistent/path/that/does/not/exist".to_string(),
            host: "127.0.0.1".to_string(),
        };
        let result = cmd.execute();
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("UI directory not found"));
    }
}
