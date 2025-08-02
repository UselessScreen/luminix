#[cfg(target_os = "windows")]
pub fn register_file_association() -> anyhow::Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
    use winreg::RegKey;
    use std::env;

    let exe_path = env::current_exe().expect("Can't get path to self");
    let exe_str = format!(r#""{}" "%1""#, exe_path.display());

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes = hkcu.open_subkey_with_flags("Software\\Classes", KEY_WRITE)?;

    // Example: Associate .png with YourApp.Image
    let (key, _) = classes.create_subkey(".png")?;
    key.set_value("", &"Luminix.Image")?;

    let (image_key, _) = classes.create_subkey("Luminix.Image\\shell\\open\\command")?;
    image_key.set_value("", &exe_str)?;
    
    Ok(())
}