use std::{
    collections::HashMap,
    iter::FromIterator,
    sync::{Arc, Mutex},
};

use sciter::Value;

use hbb_common::{
    allow_err,
    config::{LocalConfig, PeerConfig},
    log,
};

#[cfg(not(feature = "flutter"))]
use crate::ui_session_interface::Session;
use crate::{common::get_app_name, ipc, ui_interface::*};

mod cm;
#[cfg(feature = "inline")]
pub mod inline;
pub mod remote;

#[allow(dead_code)]
type Status = (i32, bool, i64, String);

lazy_static::lazy_static! {
    // stupid workaround for https://sciter.com/forums/topic/crash-on-latest-tis-mac-sdk-sometimes/
    static ref STUPID_VALUES: Mutex<Vec<Arc<Vec<Value>>>> = Default::default();
}

#[cfg(not(feature = "flutter"))]
lazy_static::lazy_static! {
    pub static ref CUR_SESSION: Arc<Mutex<Option<Session<remote::SciterHandler>>>> = Default::default();
}

struct UIHostHandler;

pub fn start(args: &mut [String]) {
    #[cfg(target_os = "macos")]
    crate::platform::delegate::show_dock();
    #[cfg(all(target_os = "linux", feature = "inline"))]
    {
        let app_dir = std::env::var("APPDIR").unwrap_or("".to_string());
        let mut so_path = "/usr/share/rustdesk/libsciter-gtk.so".to_owned();
        for (prefix, dir) in [
            ("", "/usr"),
            ("", "/app"),
            (&app_dir, "/usr"),
            (&app_dir, "/app"),
        ]
        .iter()
        {
            let path = format!("{prefix}{dir}/share/rustdesk/libsciter-gtk.so");
            if std::path::Path::new(&path).exists() {
                so_path = path;
                break;
            }
        }
        sciter::set_library(&so_path).ok();
    }
    #[cfg(windows)]
    // Check if there is a sciter.dll nearby.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let sciter_dll_path = parent.join("sciter.dll");
            if sciter_dll_path.exists() {
                // Try to set the sciter dll.
                let p = sciter_dll_path.to_string_lossy().to_string();
                log::debug!("Found dll:{}, \n {:?}", p, sciter::set_library(&p));
            }
        }
    }
    // https://github.com/c-smile/sciter-sdk/blob/master/include/sciter-x-types.h
    // https://github.com/rustdesk/rustdesk/issues/132#issuecomment-886069737
    #[cfg(windows)]
    allow_err!(sciter::set_options(sciter::RuntimeOptions::GfxLayer(
        sciter::GFX_LAYER::WARP
    )));
    use sciter::SCRIPT_RUNTIME_FEATURES::*;
    allow_err!(sciter::set_options(sciter::RuntimeOptions::ScriptFeatures(
        ALLOW_FILE_IO as u8 | ALLOW_SOCKET_IO as u8 | ALLOW_EVAL as u8 | ALLOW_SYSINFO as u8
    )));
    let mut frame = sciter::WindowBuilder::main_window().create();
    #[cfg(windows)]
    allow_err!(sciter::set_options(sciter::RuntimeOptions::UxTheming(true)));
    frame.set_title(crate::common::DIAMOND_DISPLAY_NAME);
    #[cfg(target_os = "macos")]
    crate::platform::delegate::make_menubar(frame.get_host(), args.is_empty());
    #[cfg(windows)]
    crate::platform::try_set_window_foreground(frame.get_hwnd() as _);
    let page;
    if args.len() > 1 && args[0] == "--play" {
        args[0] = "--connect".to_owned();
        let path: std::path::PathBuf = (&args[1]).into();
        let id = path
            .file_stem()
            .map(|p| p.to_str().unwrap_or(""))
            .unwrap_or("")
            .to_owned();
        args[1] = id;
    }
    if args.is_empty() {
        std::thread::spawn(move || check_zombie());
        crate::common::check_software_update();
        frame.event_handler(UI {});
        frame.sciter_handler(UIHostHandler {});
        page = "index.html";
        // Start pulse audio local server.
        #[cfg(target_os = "linux")]
        std::thread::spawn(crate::ipc::start_pa);
    } else if args[0] == "--install" {
        frame.event_handler(UI {});
        frame.sciter_handler(UIHostHandler {});
        page = "install.html";
    } else if args[0] == "--cm" {
        frame.register_behavior("connection-manager", move || {
            Box::new(cm::SciterConnectionManager::new())
        });
        page = "cm.html";
        *cm::HIDE_CM.lock().unwrap() = crate::ipc::get_config("hide_cm")
            .ok()
            .flatten()
            .unwrap_or_default()
            == "true";
    } else if (args[0] == "--connect"
        || args[0] == "--file-transfer"
        || args[0] == "--port-forward"
        || args[0] == "--rdp")
        && args.len() > 1
    {
        #[cfg(windows)]
        {
            let hw = frame.get_host().get_hwnd();
            crate::platform::windows::enable_lowlevel_keyboard(hw as _);
        }
        let mut iter = args.iter();
        let Some(cmd) = iter.next() else {
            log::error!("Failed to get cmd arg");
            return;
        };
        let cmd = cmd.to_owned();
        let Some(id) = iter.next() else {
            log::error!("Failed to get id arg");
            return;
        };
        let id = id.to_owned();
        let pass = iter.next().unwrap_or(&"".to_owned()).clone();
        let args: Vec<String> = iter.map(|x| x.clone()).collect();
        frame.set_title(&id);
        frame.register_behavior("native-remote", move || {
            let handler =
                remote::SciterSession::new(cmd.clone(), id.clone(), pass.clone(), args.clone());
            #[cfg(not(feature = "flutter"))]
            {
                *CUR_SESSION.lock().unwrap() = Some(handler.inner());
            }
            Box::new(handler)
        });
        page = "remote.html";
    } else {
        log::error!("Wrong command: {:?}", args);
        return;
    }
    #[cfg(feature = "inline")]
    {
        let html = if page == "index.html" {
            inline::get_index()
        } else if page == "cm.html" {
            inline::get_cm()
        } else if page == "install.html" {
            inline::get_install()
        } else {
            inline::get_remote()
        };
        frame.load_html(html.as_bytes(), Some(page));
    }
    #[cfg(not(feature = "inline"))]
    frame.load_file(&format!(
        "file://{}/src/ui/{}",
        std::env::current_dir()
            .map(|c| c.display().to_string())
            .unwrap_or("".to_owned()),
        page
    ));
    let hide_cm = *cm::HIDE_CM.lock().unwrap();
    if !args.is_empty() && args[0] == "--cm" && hide_cm {
        // run_app calls expand(show) + run_loop, we use collapse(hide) + run_loop instead to create a hidden window
        frame.collapse(true);
        frame.run_loop();
        return;
    }
    frame.run_app();
}

struct UI {}

impl UI {
    fn recent_sessions_updated(&self) -> bool {
        recent_sessions_updated()
    }

    fn get_id(&self) -> String {
        ipc::get_id()
    }

    fn temporary_password(&mut self) -> String {
        temporary_password()
    }

    fn update_temporary_password(&self) {
        update_temporary_password()
    }

    fn set_permanent_password(&self, password: String) {
        let _ = set_permanent_password_with_result(password);
    }

    fn is_local_permanent_password_set(&self) -> bool {
        is_local_permanent_password_set()
    }

    fn is_permanent_password_set(&self) -> bool {
        is_permanent_password_set()
    }

    fn get_remote_id(&mut self) -> String {
        LocalConfig::get_remote_id()
    }

    fn set_remote_id(&mut self, id: String) {
        LocalConfig::set_remote_id(&id);
    }

    fn goto_install(&mut self) {
        goto_install();
    }

    fn install_me(&mut self, _options: String, _path: String) {
        install_me(_options, _path, false, false);
    }

    fn update_me(&self, _path: String) {
        update_me(_path);
    }

    fn run_without_install(&self) {
        run_without_install();
    }

    fn show_run_without_install(&self) -> bool {
        show_run_without_install()
    }

    fn get_license(&self) -> String {
        get_license()
    }

    fn get_option(&self, key: String) -> String {
        get_option(key)
    }

    fn get_local_option(&self, key: String) -> String {
        get_local_option(key)
    }

    fn set_local_option(&self, key: String, value: String) {
        set_local_option(key, value);
    }

    fn peer_has_password(&self, id: String) -> bool {
        peer_has_password(id)
    }

    fn forget_password(&self, id: String) {
        forget_password(id)
    }

    fn get_peer_option(&self, id: String, name: String) -> String {
        get_peer_option(id, name)
    }

    fn set_peer_option(&self, id: String, name: String, value: String) {
        set_peer_option(id, name, value)
    }

    fn using_public_server(&self) -> bool {
        crate::using_public_server()
    }

    fn is_incoming_only(&self) -> bool {
        hbb_common::config::is_incoming_only()
    }

    pub fn is_outgoing_only(&self) -> bool {
        hbb_common::config::is_outgoing_only()
    }

    pub fn is_custom_client(&self) -> bool {
        crate::common::is_custom_client()
    }

    pub fn is_disable_settings(&self) -> bool {
        hbb_common::config::is_disable_settings()
    }

    pub fn is_disable_account(&self) -> bool {
        hbb_common::config::is_disable_account()
    }

    pub fn is_disable_installation(&self) -> bool {
        hbb_common::config::is_disable_installation()
    }

    pub fn is_disable_ab(&self) -> bool {
        hbb_common::config::is_disable_ab()
    }

    fn get_options(&self) -> Value {
        let hashmap: HashMap<String, String> =
            serde_json::from_str(&get_options()).unwrap_or_default();
        let mut m = Value::map();
        for (k, v) in hashmap {
            m.set_item(k, v);
        }
        m
    }

    fn test_if_valid_server(&self, host: String, test_with_proxy: bool) -> String {
        test_if_valid_server(host, test_with_proxy)
    }

    fn get_sound_inputs(&self) -> Value {
        Value::from_iter(get_sound_inputs())
    }

    fn set_options(&self, v: Value) {
        let mut m = HashMap::new();
        for (k, v) in v.items() {
            if let Some(k) = k.as_string() {
                if let Some(v) = v.as_string() {
                    if !v.is_empty() {
                        m.insert(k, v);
                    }
                }
            }
        }
        set_options(m);
    }

    fn set_option(&self, key: String, value: String) {
        set_option(key, value);
    }

    fn install_path(&mut self) -> String {
        install_path()
    }

    fn install_options(&self) -> String {
        install_options()
    }

    fn get_socks(&self) -> Value {
        Value::from_iter(get_socks())
    }

    fn set_socks(&self, proxy: String, username: String, password: String) {
        set_socks(proxy, username, password)
    }

    fn is_installed(&self) -> bool {
        is_installed()
    }

    fn get_supported_privacy_mode_impls(&self) -> String {
        serde_json::to_string(&crate::privacy_mode::get_supported_privacy_mode_impl())
            .unwrap_or_default()
    }

    fn is_root(&self) -> bool {
        is_root()
    }

    fn is_release(&self) -> bool {
        #[cfg(not(debug_assertions))]
        return true;
        #[cfg(debug_assertions)]
        return false;
    }

    fn is_share_rdp(&self) -> bool {
        is_share_rdp()
    }

    fn set_share_rdp(&self, _enable: bool) {
        set_share_rdp(_enable);
    }

    fn is_installed_lower_version(&self) -> bool {
        is_installed_lower_version()
    }

    fn closing(&mut self, x: i32, y: i32, w: i32, h: i32) {
        crate::server::input_service::fix_key_down_timeout_at_exit();
        LocalConfig::set_size(x, y, w, h);
    }

    fn get_size(&mut self) -> Value {
        let s = LocalConfig::get_size();
        let mut v = Vec::new();
        v.push(s.0);
        v.push(s.1);
        v.push(s.2);
        v.push(s.3);
        Value::from_iter(v)
    }

    fn get_mouse_time(&self) -> f64 {
        get_mouse_time()
    }

    fn check_mouse_time(&self) {
        check_mouse_time()
    }

    fn get_connect_status(&mut self) -> Value {
        let mut v = Value::array(0);
        let x = get_connect_status();
        v.push(x.status_num);
        v.push(x.key_confirmed);
        v.push(x.id);
        v
    }

    #[inline]
    fn get_peer_value(id: String, p: PeerConfig) -> Value {
        let values = vec![
            id,
            p.info.username.clone(),
            p.info.hostname.clone(),
            p.info.platform.clone(),
            p.options.get("alias").unwrap_or(&"".to_owned()).to_owned(),
        ];
        Value::from_iter(values)
    }

    fn get_peer(&self, id: String) -> Value {
        let c = get_peer(id.clone());
        Self::get_peer_value(id, c)
    }

    fn get_fav(&self) -> Value {
        Value::from_iter(get_fav())
    }

    fn store_fav(&self, fav: Value) {
        let mut tmp = vec![];
        fav.values().for_each(|v| {
            if let Some(v) = v.as_string() {
                if !v.is_empty() {
                    tmp.push(v);
                }
            }
        });
        store_fav(tmp);
    }

    fn get_recent_sessions(&mut self) -> Value {
        // to-do: limit number of recent sessions, and remove old peer file
        let peers: Vec<Value> = PeerConfig::peers(None)
            .drain(..)
            .map(|p| Self::get_peer_value(p.0, p.2))
            .collect();
        Value::from_iter(peers)
    }

    fn get_icon(&mut self) -> String {
        get_icon()
    }

    fn remove_peer(&mut self, id: String) {
        PeerConfig::remove(&id);
    }

    fn remove_discovered(&mut self, id: String) {
        remove_discovered(id);
    }

    fn send_wol(&mut self, id: String) {
        crate::lan::send_wol(id)
    }

    fn new_remote(&mut self, id: String, remote_type: String, force_relay: bool) {
        new_remote(id, remote_type, force_relay)
    }

    fn is_process_trusted(&mut self, _prompt: bool) -> bool {
        is_process_trusted(_prompt)
    }

    fn is_can_screen_recording(&mut self, _prompt: bool) -> bool {
        is_can_screen_recording(_prompt)
    }

    fn is_installed_daemon(&mut self, _prompt: bool) -> bool {
        is_installed_daemon(_prompt)
    }

    fn get_error(&mut self) -> String {
        get_error()
    }

    fn is_login_wayland(&mut self) -> bool {
        is_login_wayland()
    }

    fn current_is_wayland(&mut self) -> bool {
        current_is_wayland()
    }

    fn get_software_update_url(&self) -> String {
        crate::SOFTWARE_UPDATE_URL.lock().unwrap().clone()
    }

    fn get_new_version(&self) -> String {
        get_new_version()
    }

    fn get_version(&self) -> String {
        get_version()
    }

    fn get_fingerprint(&self) -> String {
        get_fingerprint()
    }

    fn get_app_name(&self) -> String {
        get_app_name()
    }

    fn get_software_ext(&self) -> String {
        #[cfg(windows)]
        let p = "exe";
        #[cfg(target_os = "macos")]
        let p = "dmg";
        #[cfg(target_os = "linux")]
        let p = "deb";
        p.to_owned()
    }

    fn get_software_store_path(&self) -> String {
        let mut p = std::env::temp_dir();
        let name = crate::SOFTWARE_UPDATE_URL
            .lock()
            .unwrap()
            .split("/")
            .last()
            .map(|x| x.to_owned())
            .unwrap_or(crate::get_app_name());
        p.push(name);
        format!("{}.{}", p.to_string_lossy(), self.get_software_ext())
    }

    fn create_shortcut(&self, _id: String) {
        #[cfg(windows)]
        create_shortcut(_id)
    }

    fn discover(&self) {
        std::thread::spawn(move || {
            allow_err!(crate::lan::discover());
        });
    }

    fn get_lan_peers(&self) -> String {
        // let peers = get_lan_peers()
        //     .into_iter()
        //     .map(|mut peer| {
        //         (
        //             peer.remove("id").unwrap_or_default(),
        //             peer.remove("username").unwrap_or_default(),
        //             peer.remove("hostname").unwrap_or_default(),
        //             peer.remove("platform").unwrap_or_default(),
        //         )
        //     })
        //     .collect::<Vec<(String, String, String, String)>>();
        serde_json::to_string(&get_lan_peers()).unwrap_or_default()
    }

    fn get_uuid(&self) -> String {
        get_uuid()
    }

    fn open_url(&self, url: String) {
        #[cfg(windows)]
        let p = "explorer";
        #[cfg(target_os = "macos")]
        let p = "open";
        #[cfg(target_os = "linux")]
        let p = if std::path::Path::new("/usr/bin/firefox").exists() {
            "firefox"
        } else {
            "xdg-open"
        };
        allow_err!(std::process::Command::new(p).arg(url).spawn());
    }

    fn change_id(&self, id: String) {
        reset_async_job_status();
        let old_id = self.get_id();
        change_id_shared(id, old_id);
    }

    fn http_request(&self, url: String, method: String, body: Option<String>, header: String) {
        http_request(url, method, body, header)
    }

    fn post_request(&self, url: String, body: String, header: String) {
        post_request(url, body, header)
    }

    fn is_ok_change_id(&self) -> bool {
        hbb_common::machine_uid::get().is_ok()
    }

    fn get_async_job_status(&self) -> String {
        get_async_job_status()
    }

    fn get_http_status(&self, url: String) -> Option<String> {
        get_async_http_status(url)
    }

    fn t(&self, name: String) -> String {
        crate::client::translate(name)
    }

    fn is_xfce(&self) -> bool {
        crate::platform::is_xfce()
    }

    fn get_api_server(&self) -> String {
        get_api_server()
    }

    fn has_hwcodec(&self) -> bool {
        has_hwcodec()
    }

    fn has_vram(&self) -> bool {
        has_vram()
    }

    fn get_langs(&self) -> String {
        get_langs()
    }

    fn video_save_directory(&self, root: bool) -> String {
        video_save_directory(root)
    }

    fn handle_relay_id(&self, id: String) -> String {
        handle_relay_id(&id).to_owned()
    }

    fn get_login_device_info(&self) -> String {
        get_login_device_info_json()
    }

    fn support_remove_wallpaper(&self) -> bool {
        support_remove_wallpaper()
    }

    fn has_valid_2fa(&self) -> bool {
        has_valid_2fa()
    }

    fn generate2fa(&self) -> String {
        generate2fa()
    }

    pub fn verify2fa(&self, code: String) -> bool {
        verify2fa(code)
    }

    fn verify_login(&self, raw: String, id: String) -> bool {
        crate::verify_login(&raw, &id)
    }

    fn generate_2fa_img_src(&self, data: String) -> String {
        let v = qrcode_generator::to_png_to_vec(data, qrcode_generator::QrCodeEcc::Low, 128)
            .unwrap_or_default();
        let s = hbb_common::sodiumoxide::base64::encode(
            v,
            hbb_common::sodiumoxide::base64::Variant::Original,
        );
        format!("data:image/png;base64,{s}")
    }

    pub fn check_hwcodec(&self) {
        check_hwcodec()
    }

    fn is_option_fixed(&self, key: String) -> bool {
        crate::ui_interface::is_option_fixed(&key)
    }

    fn get_builtin_option(&self, key: String) -> String {
        crate::ui_interface::get_builtin_option(&key)
    }

    fn is_remote_modify_enabled_by_control_permissions(&self) -> String {
        match crate::ui_interface::is_remote_modify_enabled_by_control_permissions() {
            Some(true) => "true",
            Some(false) => "false",
            None => "",
        }
        .to_string()
    }
}

impl sciter::EventHandler for UI {
    sciter::dispatch_script_call! {
        fn t(String);
        fn get_api_server();
        fn is_xfce();
        fn using_public_server();
        fn is_custom_client();
        fn is_outgoing_only();
        fn is_incoming_only();
        fn is_disable_settings();
        fn is_disable_account();
        fn is_disable_installation();
        fn is_disable_ab();
        fn get_id();
        fn temporary_password();
        fn update_temporary_password();
        fn set_permanent_password(String);
        fn is_local_permanent_password_set();
        fn is_permanent_password_set();
        fn get_remote_id();
        fn set_remote_id(String);
        fn closing(i32, i32, i32, i32);
        fn get_size();
        fn new_remote(String, String, bool);
        fn send_wol(String);
        fn remove_peer(String);
        fn remove_discovered(String);
        fn get_connect_status();
        fn get_mouse_time();
        fn check_mouse_time();
        fn get_recent_sessions();
        fn get_peer(String);
        fn get_fav();
        fn store_fav(Value);
        fn recent_sessions_updated();
        fn get_icon();
        fn install_me(String, String);
        fn is_installed();
        fn get_supported_privacy_mode_impls();
        fn is_root();
        fn is_release();
        fn set_socks(String, String, String);
        fn get_socks();
        fn is_share_rdp();
        fn set_share_rdp(bool);
        fn is_installed_lower_version();
        fn install_path();
        fn install_options();
        fn goto_install();
        fn is_process_trusted(bool);
        fn is_can_screen_recording(bool);
        fn is_installed_daemon(bool);
        fn get_error();
        fn is_login_wayland();
        fn current_is_wayland();
        fn get_options();
        fn get_option(String);
        fn get_local_option(String);
        fn set_local_option(String, String);
        fn get_peer_option(String, String);
        fn peer_has_password(String);
        fn forget_password(String);
        fn set_peer_option(String, String, String);
        fn get_license();
        fn test_if_valid_server(String, bool);
        fn get_sound_inputs();
        fn set_options(Value);
        fn set_option(String, String);
        fn get_software_update_url();
        fn get_new_version();
        fn get_version();
        fn get_fingerprint();
        fn update_me(String);
        fn show_run_without_install();
        fn run_without_install();
        fn get_app_name();
        fn get_software_store_path();
        fn get_software_ext();
        fn open_url(String);
        fn change_id(String);
        fn get_async_job_status();
        fn post_request(String, String, String);
        fn is_ok_change_id();
        fn create_shortcut(String);
        fn discover();
        fn get_lan_peers();
        fn get_uuid();
        fn has_hwcodec();
        fn has_vram();
        fn get_langs();
        fn video_save_directory(bool);
        fn handle_relay_id(String);
        fn get_login_device_info();
        fn support_remove_wallpaper();
        fn has_valid_2fa();
        fn generate2fa();
        fn generate_2fa_img_src(String);
        fn verify2fa(String);
        fn check_hwcodec();
        fn verify_login(String, String);
        fn is_option_fixed(String);
        fn get_builtin_option(String);
        fn is_remote_modify_enabled_by_control_permissions();
    }
}

impl sciter::host::HostHandler for UIHostHandler {
    fn on_graphics_critical_failure(&mut self) {
        log::error!("Critical rendering error: e.g. DirectX gfx driver error. Most probably bad gfx drivers.");
    }
}

#[cfg(not(target_os = "linux"))]
fn get_sound_inputs() -> Vec<String> {
    let mut out = Vec::new();
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    if let Ok(devices) = host.devices() {
        for device in devices {
            if device.default_input_config().is_err() {
                continue;
            }
            if let Ok(name) = device.name() {
                out.push(name);
            }
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn get_sound_inputs() -> Vec<String> {
    crate::platform::linux::get_pa_sources()
        .drain(..)
        .map(|x| x.1)
        .collect()
}

// sacrifice some memory
pub fn value_crash_workaround(values: &[Value]) -> Arc<Vec<Value>> {
    let persist = Arc::new(values.to_vec());
    STUPID_VALUES.lock().unwrap().push(persist.clone());
    persist
}

pub fn get_icon() -> String {
    // 128x128
    #[cfg(target_os = "macos")]
    // 128x128 on 160x160 canvas, then shrink to 128, mac looks better with padding
    {
        "data:image/png;base64,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".into()
    }
    #[cfg(not(target_os = "macos"))] // 128x128 no padding
    {
        "data:image/png;base64,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".into()
    }
}
