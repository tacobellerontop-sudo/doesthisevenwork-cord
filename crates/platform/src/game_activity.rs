//! Opt-in Discord-compatible local IPC transport. No Discord connection; process detection
//! for games without RPC lives in `platform::processes`.
//! The caller owns framing, timeouts, client limits and activity publication.

pub use native::{Listener, Stream};

mod native {
	use std::{
		io,
		pin::Pin,
		task::{Context, Poll},
	};
	use tokio::{
		io::{AsyncRead, AsyncWrite, ReadBuf},
		net::windows::named_pipe::{NamedPipeServer, ServerOptions},
	};

	pub struct Stream(NamedPipeServer);

	impl Drop for Stream {
		fn drop(&mut self) {
			// Mio otherwise preserves pending writes on drop, allowing a non-reading
			// client to retain the pipe after sharing is disabled or the session ends.
			let _ = self.0.disconnect();
		}
	}

	impl AsyncRead for Stream {
		fn poll_read(
			mut self: Pin<&mut Self>,
			cx: &mut Context<'_>,
			buf: &mut ReadBuf<'_>,
		) -> Poll<io::Result<()>> {
			Pin::new(&mut self.0).poll_read(cx, buf)
		}
	}

	impl AsyncWrite for Stream {
		fn poll_write(
			mut self: Pin<&mut Self>,
			cx: &mut Context<'_>,
			buf: &[u8],
		) -> Poll<io::Result<usize>> {
			Pin::new(&mut self.0).poll_write(cx, buf)
		}
		fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
			Pin::new(&mut self.0).poll_flush(cx)
		}
		fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
			Pin::new(&mut self.0).poll_shutdown(cx)
		}
	}

	pub struct Listener {
		server: Stream,
		name: String,
		dacl: Vec<u16>,
	}

	impl Listener {
		/// Bind the first free standard slot without joining another server's pipe.
		/// Requires an entered Tokio runtime with I/O enabled (including spawn_blocking).
		pub fn bind() -> io::Result<Self> {
			let mut last = None;
			for index in 0..10 {
				match Self::bind_named(&format!(r"\\.\pipe\discord-ipc-{index}")) {
					Ok(listener) => return Ok(listener),
					Err(error) => last = Some(error),
				}
			}
			Err(last.expect("ten slots were attempted"))
		}

		fn bind_named(name: &str) -> io::Result<Self> {
			let dacl = current_user_dacl()?;
			let server = create_pipe(name, &dacl, true)?;
			Ok(Self {
				server,
				name: name.into(),
				dacl,
			})
		}

		pub async fn accept(&mut self) -> io::Result<Stream> {
			self.server.0.connect().await?;
			// Keep this owned instance alive while reserving the next one: no name-hijack gap.
			let next = create_pipe(&self.name, &self.dacl, false)?;
			Ok(std::mem::replace(&mut self.server, next))
		}
	}

	#[allow(unsafe_code)]
	fn current_user_dacl() -> io::Result<Vec<u16>> {
		use windows::{
			Win32::{
				Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree},
				Security::{
					Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY,
					TOKEN_USER, TokenUser,
				},
				System::Threading::{GetCurrentProcess, OpenProcessToken},
			},
			core::PWSTR,
		};
		let mut token = HANDLE::default();
		// SAFETY: output is a valid handle slot; only the current process token is queried.
		unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
			.map_err(io::Error::other)?;
		// TOKEN_USER plus the maximum-size SID fit in this bounded, pointer-aligned buffer.
		let mut storage = [0usize; 128];
		let mut length = 0;
		// SAFETY: buffer is writable and aligned for TOKEN_USER, with its actual byte size.
		let result = unsafe {
			GetTokenInformation(
				token,
				TokenUser,
				Some(storage.as_mut_ptr().cast()),
				std::mem::size_of_val(&storage) as u32,
				&mut length,
			)
		};
		// SAFETY: this function owns the successful OpenProcessToken result.
		let _ = unsafe { CloseHandle(token) };
		result.map_err(io::Error::other)?;
		// SAFETY: successful GetTokenInformation initialized TOKEN_USER and its in-buffer SID.
		let user = unsafe { &*storage.as_ptr().cast::<TOKEN_USER>() };
		let mut sid = PWSTR::null();
		// SAFETY: SID remains backed by storage; Windows allocates the output string.
		unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid) }.map_err(io::Error::other)?;
		// SAFETY: successful conversion returned a NUL-terminated UTF-16 string.
		let text = unsafe { sid.to_string() };
		// SAFETY: ConvertSidToStringSidW requires LocalFree for its allocation.
		unsafe { LocalFree(Some(HLOCAL(sid.0.cast()))) };
		let text = text.map_err(io::Error::other)?;
		Ok(format!("D:P(A;;GA;;;{text})")
			.encode_utf16()
			.chain([0])
			.collect())
	}

	#[allow(unsafe_code)]
	fn create_pipe(name: &str, dacl: &[u16], first: bool) -> io::Result<Stream> {
		use windows::{
			Win32::{
				Foundation::{HLOCAL, LocalFree},
				Security::{
					Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
					PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
				},
			},
			core::PCWSTR,
		};
		let mut descriptor = PSECURITY_DESCRIPTOR::default();
		// SAFETY: dacl is the owned NUL-terminated SDDL constructed above; output is writable.
		unsafe {
			ConvertStringSecurityDescriptorToSecurityDescriptorW(
				PCWSTR(dacl.as_ptr()),
				1,
				&mut descriptor,
				None,
			)
		}
		.map_err(io::Error::other)?;
		let mut attributes = SECURITY_ATTRIBUTES {
			nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
			lpSecurityDescriptor: descriptor.0,
			bInheritHandle: false.into(),
		};
		// SAFETY: attributes and its descriptor remain valid for synchronous pipe creation.
		let result = unsafe {
			ServerOptions::new()
				.first_pipe_instance(first)
				.reject_remote_clients(true)
				.max_instances(9) // Eight active clients plus the next reserved listener.
				.in_buffer_size(16 * 1024)
				.out_buffer_size(16 * 1024)
				.create_with_security_attributes_raw(
					name,
					(&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
				)
		};
		// SAFETY: descriptor was allocated by the conversion function and is no longer needed.
		unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
		result.map(Stream)
	}

	#[cfg(test)]
	mod tests {
		use super::*;
		use tokio::{
			io::{AsyncReadExt, AsyncWriteExt},
			net::windows::named_pipe::ClientOptions,
		};

		#[tokio::test]
		async fn private_pipe_roundtrip_contention_and_release() {
			let name = format!(
				r"\\.\pipe\dtewcord-test-{}-{}",
				std::process::id(),
				getrandom::u64().unwrap()
			);
			let mut listener = Listener::bind_named(&name).unwrap();
			assert!(Listener::bind_named(&name).is_err());
			for _ in 0..2 {
				let mut client = ClientOptions::new().open(&name).unwrap();
				let mut server =
					tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept())
						.await
						.unwrap()
						.unwrap();
				client.write_all(b"synthetic").await.unwrap();
				let mut bytes = [0; 9];
				tokio::time::timeout(
					std::time::Duration::from_secs(2),
					server.read_exact(&mut bytes),
				)
				.await
				.unwrap()
				.unwrap();
				assert_eq!(&bytes, b"synthetic");
				server.write_all(b"reply").await.unwrap();
				let mut reply = [0; 5];
				tokio::time::timeout(
					std::time::Duration::from_secs(2),
					client.read_exact(&mut reply),
				)
				.await
				.unwrap()
				.unwrap();
				assert_eq!(&reply, b"reply");
				drop(server);
				drop(client);
				assert!(Listener::bind_named(&name).is_err());
			}
			// Keep a client alive without reading until its server-side output stalls.
			let idle_client = ClientOptions::new().open(&name).unwrap();
			let mut blocked = listener.accept().await.unwrap();
			assert!(
				tokio::time::timeout(std::time::Duration::from_millis(50), async {
					for _ in 0..8 {
						blocked.write_all(&[0; 16 * 1024]).await.unwrap();
					}
				})
				.await
				.is_err()
			);
			drop(blocked);
			drop(listener);
			// Mio cancels overlapped reads on drop; their IOCP completions release the
			// final handle references when the runtime next polls its I/O driver.
			tokio::time::timeout(std::time::Duration::from_secs(2), async {
				loop {
					match Listener::bind_named(&name) {
						Ok(rebound) => break rebound,
						Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
							tokio::time::sleep(std::time::Duration::from_millis(1)).await;
						}
						Err(error) => panic!("rebind failed: {error}"),
					}
				}
			})
			.await
			.expect("all owned pipe handles must release after I/O cancellation completes");
			drop(idle_client);
		}
	}
}
