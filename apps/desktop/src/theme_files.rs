//! Native dialogs for the theme picker: importing and exporting custom theme files.
use eframe::egui;
use std::{path::PathBuf, sync::Arc, sync::mpsc};
use ui::themes::{FileRequest, Themes};

/// Where custom themes live: `<data dir>/themes`. Demo runs use a throwaway folder so they
/// never touch the owner's themes.
pub fn library_dir(demo: bool) -> Option<PathBuf> {
	if demo {
		Some(std::env::temp_dir().join("dtewcord-demo-themes"))
	} else {
		local_store::data_dir()
			.ok()
			.map(|root| root.join(theme::LIBRARY_DIR))
	}
}

/// At most one theme dialog is open at a time.
#[derive(Default)]
pub struct ThemeFiles {
	pending: Option<(FileRequest, mpsc::Receiver<Option<PathBuf>>)>,
}

impl ThemeFiles {
	pub fn tick(
		&mut self,
		themes: &mut Themes,
		ctx: &egui::Context,
		runtime: &tokio::runtime::Runtime,
		window: &Arc<winit::window::Window>,
	) {
		if let Some((request, receive)) = &self.pending {
			match receive.try_recv() {
				Ok(Some(path)) => {
					match request {
						FileRequest::Import => themes.import_file(ctx, &path),
						FileRequest::Export { id, .. } => themes.export_file(id, &path),
						FileRequest::OpenFolder(_) => {}
					}
					self.pending = None;
				}
				Ok(None) | Err(mpsc::TryRecvError::Disconnected) => self.pending = None,
				Err(mpsc::TryRecvError::Empty) => return,
			}
		}
		let Some(request) = themes.file_request.take() else {
			return;
		};
		let (send, receive) = mpsc::sync_channel(1);
		let ctx = ctx.clone();
		match &request {
			FileRequest::Import => {
				let future = platform::save::theme_file_source(window.clone());
				runtime.spawn(async move {
					let _ = send.send(future.await);
					ctx.request_repaint();
				});
			}
			FileRequest::Export { name, .. } => {
				let future =
					platform::save::theme_file_destination(window.clone(), &format!("{name}.json"));
				runtime.spawn(async move {
					let _ = send.send(future.await);
					ctx.request_repaint();
				});
			}
			FileRequest::OpenFolder(folder) => {
				// When the system refuses the shell, the path still has to reach the owner.
				if platform::open_folder(folder).is_err() {
					ctx.copy_text(folder.display().to_string());
				}
				return;
			}
		}
		self.pending = Some((request, receive));
	}
}
