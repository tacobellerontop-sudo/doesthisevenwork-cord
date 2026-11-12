fn main() {
	slint_build::compile("ui/app.slint").expect("Slint UI should compile");
	if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
		windows_icon();
	}
}

/// Embed the application icon so Explorer, shortcuts and the taskbar show it for the executable.
fn windows_icon() {
	use std::{path::PathBuf, process::Command};
	println!("cargo:rerun-if-changed=../../packaging/windows/DTEW Cord.ico");
	println!("cargo:rerun-if-changed=../../packaging/windows/dtewcord-slint.rc");
	let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
		.join("../../packaging/windows");
	let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
	let (mut compiler, resource) = if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
	{
		let host = std::env::var("HOST").unwrap();
		let sdk = find_msvc_tools::find_windows_sdk(host.split('-').next().unwrap())
			.expect("Windows SDK is required to embed the application icon");
		let rc = sdk
			.path()
			.map(|path| path.join("rc.exe"))
			.find(|path| path.is_file())
			.expect("Windows SDK rc.exe");
		let resource = out.join("dtewcord-slint.res");
		let mut compiler = Command::new(rc);
		compiler
			.arg("/nologo")
			.arg("/fo")
			.arg(&resource)
			.arg("dtewcord-slint.rc");
		(compiler, resource)
	} else {
		let resource = out.join("dtewcord-slint-icon.o");
		let mut compiler = Command::new("windres");
		compiler
			.args(["-i", "dtewcord-slint.rc", "-o"])
			.arg(&resource);
		(compiler, resource)
	};
	assert!(
		compiler
			.current_dir(root)
			.status()
			.expect("run icon resource compiler")
			.success(),
		"application icon resource compilation failed"
	);
	println!(
		"cargo:rustc-link-arg-bin=dtewcord-slint={}",
		resource.display()
	);
}
