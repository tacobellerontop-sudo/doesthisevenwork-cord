# Adds a "DTEW Cord" Start Menu shortcut that carries the app's AppUserModelID, which Windows
# needs before it shows toast notifications for an unpackaged app. Run it from the extracted
# release folder:  powershell -ExecutionPolicy Bypass -File .\install-notifications.ps1
$ErrorActionPreference = 'Stop'
$appId = 'io.github.tacobellerontop_sudo.dtewcord'
$exe = Join-Path $PSScriptRoot 'dtewcord.exe'
if (-not (Test-Path $exe)) { throw "dtewcord.exe was not found next to this script." }
$link = Join-Path ([Environment]::GetFolderPath('Programs')) 'DTEW Cord.lnk'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class DtewCordShortcut {
	[ComImport, Guid("000214F9-0000-0000-C000-000000000046"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
	interface IShellLinkW {
		void GetPath(IntPtr a, int b, IntPtr c, int d); void GetIDList(out IntPtr a); void SetIDList(IntPtr a);
		void GetDescription(IntPtr a, int b); void SetDescription([MarshalAs(UnmanagedType.LPWStr)] string a);
		void GetWorkingDirectory(IntPtr a, int b); void SetWorkingDirectory([MarshalAs(UnmanagedType.LPWStr)] string a);
		void GetArguments(IntPtr a, int b); void SetArguments([MarshalAs(UnmanagedType.LPWStr)] string a);
		void GetHotkey(out short a); void SetHotkey(short a); void GetShowCmd(out int a); void SetShowCmd(int a);
		void GetIconLocation(IntPtr a, int b, out int c); void SetIconLocation([MarshalAs(UnmanagedType.LPWStr)] string a, int b);
		void SetRelativePath([MarshalAs(UnmanagedType.LPWStr)] string a, int b); void Resolve(IntPtr a, int b);
		void SetPath([MarshalAs(UnmanagedType.LPWStr)] string a);
	}
	[ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
	interface IPropertyStore {
		void GetCount(out uint a); void GetAt(uint a, out PropertyKey b); void GetValue(ref PropertyKey a, IntPtr b);
		void SetValue(ref PropertyKey key, PropVariant value); void Commit();
	}
	[ComImport, Guid("0000010b-0000-0000-C000-000000000046"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
	interface IPersistFile {
		void GetClassID(out Guid a); [PreserveSig] int IsDirty();
		void Load([MarshalAs(UnmanagedType.LPWStr)] string a, uint b);
		void Save([MarshalAs(UnmanagedType.LPWStr)] string file, bool remember);
	}
	[StructLayout(LayoutKind.Sequential, Pack = 4)]
	struct PropertyKey { public Guid fmtid; public uint pid; }
	[StructLayout(LayoutKind.Explicit)]
	class PropVariant { [FieldOffset(0)] public ushort vt; [FieldOffset(8)] public IntPtr value; }
	[ComImport, Guid("00021401-0000-0000-C000-000000000046")] class ShellLink {}

	public static void Create(string link, string exe, string appId) {
		var shell = (IShellLinkW)new ShellLink();
		shell.SetPath(exe);
		shell.SetWorkingDirectory(System.IO.Path.GetDirectoryName(exe));
		shell.SetIconLocation(exe, 0);
		shell.SetDescription("DTEW Cord");
		var key = new PropertyKey { fmtid = new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"), pid = 5 };
		var value = new PropVariant { vt = 31, value = Marshal.StringToCoTaskMemUni(appId) };
		try {
			var store = (IPropertyStore)shell;
			store.SetValue(ref key, value);
			store.Commit();
		} finally {
			Marshal.FreeCoTaskMem(value.value);
		}
		((IPersistFile)shell).Save(link, true);
	}
}
'@

[DtewCordShortcut]::Create($link, $exe, $appId)
Write-Host "Created $link. DTEW Cord can now show Windows notifications."
