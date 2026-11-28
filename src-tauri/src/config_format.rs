//! 可迁移配置的白名单。不会序列化翻译、凭据、历史、正文或系统联动开关。
use crate::{applications::AppConfig, state::{UiPreferences, UiState}};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_BYTES: usize = 2 * 1024 * 1024;
pub struct PendingConfiguration {
    pub token: String,
    pub backup: ConfigurationBackup,
    pub fingerprint: String,
    pub created: std::time::Instant,
}
impl PendingConfiguration {
    pub fn validate_confirmation(&self, token: &str, current: &ConfigurationBackup) -> Result<(), String> {
        if self.token != token || self.created.elapsed() > std::time::Duration::from_secs(600) { return Err("导入预览已失效，请重新选择文件".into()); }
        if current.fingerprint()? != self.fingerprint { return Err("预览后配置已变化，请重新选择文件核对差异".into()); }
        Ok(())
    }
}
fn default_telemetry_mode() -> String {
    "smart".into()
}
fn default_telemetry_style() -> String {
    "bar".into()
}
fn default_motion_mode() -> String {
    "glide".into()
}
fn default_island_shape() -> String {
    crate::state::ISLAND_SHAPE_FLOATING.into()
}
fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PortablePreferences {
    pub theme: String,
    pub active_category_id: String,
    pub island_anchor: String,
    #[serde(default = "default_island_shape")]
    pub island_shape: String,
    pub island_page: String,
    pub island_hover_ms: u32,
    pub island_leave_ms: u32,
    pub island_reduced_motion: bool,
    pub scale: u32,
    pub webgl: bool,
    #[serde(default = "default_motion_mode")]
    pub motion: String,
    #[serde(default = "default_telemetry_mode")]
    pub telemetry_mode: String,
    #[serde(default = "default_telemetry_style")]
    pub telemetry_style: String,
    #[serde(default = "default_true")]
    pub telemetry_net: bool,
    #[serde(default = "default_true")]
    pub telemetry_cpu: bool,
    #[serde(default = "default_true")]
    pub telemetry_gpu: bool,
    #[serde(default = "default_true")]
    pub telemetry_memory: bool,
    #[serde(default = "default_true")]
    pub telemetry_peripherals: bool,
    #[serde(default = "default_true")]
    pub show_app_names: bool,
}
impl PortablePreferences {
    pub fn capture(ui: &UiState) -> Self {
        Self {
            theme: ui.theme.clone(),
            active_category_id: ui.active_category_id.clone(),
            island_anchor: ui.island_anchor.clone(),
            island_shape: crate::state::normalize_island_shape(&ui.island_shape),
            island_page: if crate::state::valid_island_page(&ui.island_page) { ui.island_page.clone() } else { crate::state::PANEL_APPLICATIONS.into() },
            island_hover_ms: ui.island_hover_ms,
            island_leave_ms: ui.island_leave_ms,
            island_reduced_motion: ui.island_reduced_motion,
            scale: ui.island_appearance.scale,
            webgl: ui.island_appearance.webgl,
            motion: ui.island_appearance.motion.clone(),
            telemetry_mode: ui.island_appearance.telemetry_mode.clone(),
            telemetry_style: ui.island_appearance.telemetry_style.clone(),
            telemetry_net: ui.island_appearance.telemetry_net,
            telemetry_cpu: ui.island_appearance.telemetry_cpu,
            telemetry_gpu: ui.island_appearance.telemetry_gpu,
            telemetry_memory: ui.island_appearance.telemetry_memory,
            telemetry_peripherals: ui.island_appearance.telemetry_peripherals,
            show_app_names: ui.island_appearance.show_app_names,
        }
    }
    pub fn apply(&self, ui: &mut UiState) {
        ui.theme = self.theme.clone(); ui.active_category_id = self.active_category_id.clone();
        ui.island_anchor = self.island_anchor.clone();
        ui.island_shape = crate::state::normalize_island_shape(&self.island_shape);
        ui.island_page = self.island_page.clone();
        ui.island_hover_ms = self.island_hover_ms; ui.island_leave_ms = self.island_leave_ms;
        ui.island_reduced_motion = self.island_reduced_motion;
        ui.island_appearance.scale = self.scale; ui.island_appearance.webgl = self.webgl;
        ui.island_appearance.motion = self.motion.clone();
        ui.island_appearance.telemetry_mode = self.telemetry_mode.clone();
        ui.island_appearance.telemetry_style = self.telemetry_style.clone();
        ui.island_appearance.telemetry_net = self.telemetry_net;
        ui.island_appearance.telemetry_cpu = self.telemetry_cpu;
        ui.island_appearance.telemetry_gpu = self.telemetry_gpu;
        ui.island_appearance.telemetry_memory = self.telemetry_memory;
        ui.island_appearance.telemetry_peripherals = self.telemetry_peripherals;
        ui.island_appearance.show_app_names = self.show_app_names;
        if ui.island_expanded { ui.panel = if self.island_page == "applications" { None } else { Some(self.island_page.clone()) }; }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ConfigurationBackup {
    pub format: String,
    pub version: u32,
    pub applications: AppConfig,
    pub preferences: PortablePreferences,
}
impl ConfigurationBackup {
    pub fn capture(apps: &AppConfig, ui: &UiState) -> Self {
        let mut applications = apps.clone();
        // Older lists display empty/deleted category references as uncategorized.
        // Normalize the exported copy only; importing arbitrary missing IDs stays invalid.
        if !applications.categories.iter().any(|category| category.id == "all") {
            applications.categories.insert(0, crate::applications::Category { id: "all".into(), name: "全部".into() });
        }
        for app in &mut applications.apps {
            app.broken = None;
            if !applications.categories.iter().any(|category| category.id == app.category_id) { app.category_id = "all".into(); }
        }
        let mut preferences = PortablePreferences::capture(ui);
        if !applications.categories.iter().any(|category| category.id == preferences.active_category_id) { preferences.active_category_id = "all".into(); }
        Self { format: "desktophub-lite-config".into(), version: 1, applications, preferences }
    }
    pub fn fingerprint(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(self).map_err(|_| "无法读取当前配置")?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
    pub fn merged_preferences(&self, ui: &UiState) -> UiPreferences {
        let mut next = ui.clone(); self.preferences.apply(&mut next); UiPreferences::from_state(&next)
    }
}
pub fn parse(bytes: &[u8]) -> Result<ConfigurationBackup, String> {
    if bytes.len() > MAX_BYTES { return Err("配置文件超过 2 MiB".into()); }
    let mut backup: ConfigurationBackup = serde_json::from_slice(bytes).map_err(|_| "配置格式损坏、字段缺失或包含未支持的字段".to_string())?;
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| "不是有效的 JSON 配置")?;
    fn keys(value: &serde_json::Value, allowed: &[&str]) -> Result<(), String> {
        let object = value.as_object().ok_or("配置结构无效")?;
        if object.keys().any(|key| !allowed.contains(&key.as_str())) { return Err("应用配置中存在未支持的字段，已停止导入".into()); }
        Ok(())
    }
    keys(&value["applications"], &["schemaVersion", "categories", "apps"])?;
    for category in value["applications"]["categories"].as_array().ok_or("分类结构无效")? { keys(category, &["id", "name"])?; }
    for app in value["applications"]["apps"].as_array().ok_or("应用结构无效")? { keys(app, &["id", "name", "target", "categoryId", "color", "initials", "hidden", "broken"])?; }
    // 兼容旧版备份的已移除页面；其余未知页面仍严格拒绝。
    if backup.preferences.island_page == "compare" { backup.preferences.island_page = crate::state::PANEL_APPLICATIONS.into(); }
    // 兼容旧版备份中已精简移除的琉璃、书卷、黏土主题，导入时平滑降级。
    if matches!(backup.preferences.theme.as_str(), "liquid" | "scroll" | "clay") {
        backup.preferences.theme = crate::state::normalize_theme(&backup.preferences.theme);
    }
    validate(&backup)?;
    Ok(backup)
}

pub fn validate(backup: &ConfigurationBackup) -> Result<(), String> {
    if backup.format != "desktophub-lite-config" || backup.version != 1 || backup.applications.schema_version != crate::applications::SCHEMA_VERSION { return Err("配置版本不支持，请使用兼容版本的备份".into()); }
    let apps = &backup.applications;
    if apps.apps.len() > 5000 || apps.categories.len() > 200 { return Err("最多恢复 5000 个应用和 200 个分类".into()); }
    fn valid_text(text: &str, max: usize) -> bool { !text.trim().is_empty() && text.chars().count() <= max && !text.chars().any(char::is_control) }
    let mut categories = std::collections::HashSet::new();
    for category in &apps.categories {
        if !valid_text(&category.id, 128) || !valid_text(&category.name, 40) || !categories.insert(category.id.as_str()) { return Err("分类 ID 重复或名称无效".into()); }
    }
    if !categories.contains("all") { return Err("配置缺少默认分类 all".into()); }
    let mut ids = std::collections::HashSet::new();
    for app in &apps.apps {
        if !valid_text(&app.id, 128) || !ids.insert(app.id.as_str()) { return Err("应用 ID 为空、过长或重复".into()); }
        if !valid_text(&app.name, 200) || !valid_text(&app.target, 8192) || app.initials.chars().count() > 8 { return Err(format!("应用名称或目标无效：{}", app.id)); }
        if !categories.contains(app.category_id.as_str()) { return Err(format!("应用引用了不存在的分类：{}", app.name)); }
        if !matches!(app.color.len(), 4 | 7 | 9) || !app.color.starts_with('#') || !app.color[1..].chars().all(|c| c.is_ascii_hexdigit()) { return Err(format!("应用颜色无效：{}", app.name)); }
    }
    let p = &backup.preferences;
    if !crate::state::valid_theme(&p.theme)
        || !matches!(p.island_anchor.as_str(), "left" | "center" | "right")
        || !crate::state::valid_island_shape(&p.island_shape)
        || !crate::state::valid_island_page(&p.island_page)
        || !(75..=125).contains(&p.scale)
        || !(100..=2000).contains(&p.island_hover_ms)
        || !(100..=4000).contains(&p.island_leave_ms)
        || !categories.contains(p.active_category_id.as_str())
        || !crate::desktop_features::IslandAppearance::valid_motion_mode(&p.motion)
        || !crate::desktop_features::IslandAppearance::valid_telemetry_mode(&p.telemetry_mode)
        || !crate::desktop_features::IslandAppearance::valid_telemetry_style(&p.telemetry_style)
        || (p.telemetry_mode != "off" && !p.telemetry_net && !p.telemetry_cpu && !p.telemetry_gpu && !p.telemetry_memory)
    {
        return Err("显示偏好包含无效主题、页面、分类、比例、延时或胶囊微读数设置".into());
    }
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeCounts { pub before: usize, pub after: usize, pub added: usize, pub removed: usize, pub changed: usize, pub reordered: bool }
#[derive(Clone, Serialize)]
pub struct PreferenceChange { pub label: String, pub before: String, pub after: String }
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes { pub apps: ChangeCounts, pub categories: ChangeCounts, pub preferences: Vec<PreferenceChange>, pub app_details: Vec<String>, pub category_details: Vec<String> }

pub fn mark_targets(backup: &mut ConfigurationBackup, exists: impl Fn(&str) -> bool) -> Vec<String> {
    let mut warnings = Vec::new();
    for item in &mut backup.applications.apps {
        let target = item.target.trim();
        let network = target.starts_with("\\\\") || target.starts_with("//");
        let available = !network && exists(target);
        item.broken = if available { None } else { Some(true) };
        if !available { warnings.push(format!("{}：{}", item.name, if network { "网络路径未验证，请恢复后手动检查" } else { "本机找不到目标，请恢复后编辑路径" })); }
    }
    warnings
}
pub fn describe(before: &ConfigurationBackup, after: &ConfigurationBackup) -> Changes {
    fn counts<T: Serialize>(a: &[T], b: &[T], id: impl Fn(&T) -> &str) -> ChangeCounts {
        let added=b.iter().filter(|v| !a.iter().any(|old| id(old)==id(v))).count();
        let removed=a.iter().filter(|v| !b.iter().any(|new| id(new)==id(v))).count();
        let changed=b.iter().filter(|v| a.iter().find(|old| id(old)==id(v)).is_some_and(|old| serde_json::to_value(old).ok()!=serde_json::to_value(v).ok())).count();
        ChangeCounts{before:a.len(),after:b.len(),added,removed,changed,reordered:a.iter().map(&id).filter(|key| b.iter().any(|v| id(v)==*key)).collect::<Vec<_>>() != b.iter().map(&id).filter(|key| a.iter().any(|v| id(v)==*key)).collect::<Vec<_>>()}
    }
    let apps=counts(&before.applications.apps,&after.applications.apps,|v| &v.id);
    let categories=counts(&before.applications.categories,&after.applications.categories,|v| &v.id);
    let mut preferences=Vec::new();
    let a=serde_json::to_value(&before.preferences).unwrap_or_default();let b=serde_json::to_value(&after.preferences).unwrap_or_default();
    for (key,label) in [("theme","显示主题"),("motion","动态效果"),("islandShape","灵动岛形态"),("activeCategoryId","当前分类"),("islandAnchor","停靠位置"),("islandPage","灵动岛页面"),("islandHoverMs","展开延时"),("islandLeaveMs","收起延时"),("islandReducedMotion","减少动画"),("scale","显示大小"),("webgl","边缘光效"),("telemetryMode","胶囊微读数"),("telemetryStyle","微读数样式"),("telemetryNet","微读数·网速"),("telemetryCpu","微读数·处理器"),("telemetryGpu","微读数·显卡"),("telemetryMemory","微读数·内存"),("telemetryPeripherals","键鼠电量显示")] {
        if a[key]!=b[key] { preferences.push(PreferenceChange{label:label.into(),before:a[key].as_str().map(str::to_owned).unwrap_or_else(||a[key].to_string()),after:b[key].as_str().map(str::to_owned).unwrap_or_else(||b[key].to_string())}); }
    }
    let mut app_details=Vec::new();
    for new in &after.applications.apps {
        match before.applications.apps.iter().find(|old|old.id==new.id) {
            None=>app_details.push(format!("新增：{} → {}（分类 {}，{}）",new.name,new.target,new.category_id,if new.hidden { "隐藏" } else { "显示" })),
            Some(old) if serde_json::to_value(old).ok()!=serde_json::to_value(new).ok()=> {
                let mut details=Vec::new();
                if old.name!=new.name { details.push(format!("名称 {} → {}",old.name,new.name)); }
                if old.target!=new.target { details.push(format!("目标 {} → {}",old.target,new.target)); }
                if old.category_id!=new.category_id { details.push(format!("分类 {} → {}",old.category_id,new.category_id)); }
                if old.hidden!=new.hidden { details.push(if new.hidden { "改为隐藏".into() } else { "改为显示".into() }); }
                if old.color!=new.color || old.initials!=new.initials { details.push("图标颜色或缩写变化".into()); }
                app_details.push(format!("修改 {}：{}",new.name,details.join("；")));
            },
            _=>{}
        }
    }
    for old in &before.applications.apps { if !after.applications.apps.iter().any(|new|new.id==old.id) { app_details.push(format!("移出清单：{}（不删除原文件）",old.name)); } }
    let mut category_details=Vec::new();
    for new in &after.applications.categories {
        match before.applications.categories.iter().find(|old|old.id==new.id) {
            None=>category_details.push(format!("新增：{}（{}）",new.name,new.id)),
            Some(old) if old.name!=new.name=>category_details.push(format!("改名：{} → {}（{}）",old.name,new.name,new.id)),
            _=>{}
        }
    }
    for old in &before.applications.categories { if !after.applications.categories.iter().any(|new|new.id==old.id) { category_details.push(format!("移出：{}（{}）",old.name,old.id)); } }
    Changes{apps,categories,preferences,app_details,category_details}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::applications::{Category, HubApp};
    pub fn sample() -> ConfigurationBackup {
        let apps = AppConfig { schema_version: 1, categories: vec![Category{id:"all".into(),name:"全部".into()},Category{id:"dev".into(),name:"开发".into()}], apps: vec![HubApp{id:"a".into(),name:"示例".into(),target:"D:\\example.exe".into(),category_id:"dev".into(),color:"#456789".into(),initials:"例".into(),hidden:false,broken:Some(true)}] };
        ConfigurationBackup::capture(&apps, &UiState::default())
    }
    fn bytes(value: &ConfigurationBackup) -> Vec<u8> { serde_json::to_vec(value).unwrap() }
    #[test] fn round_trip_preserves_portable_configuration() {
        for theme in ["midnight", "mecha", "pixel", "blueprint", "neon"] {
            let mut backup=sample(); backup.preferences.theme=theme.into();
            let restored=parse(&bytes(&backup)).unwrap(); assert_eq!(backup.fingerprint().unwrap(),restored.fingerprint().unwrap()); assert!(restored.applications.apps[0].broken.is_none()); assert_eq!(restored.preferences.theme,theme);
        }
        for (legacy_theme, fallback) in [("liquid", "dark"), ("scroll", "light"), ("clay", "light")] {
            let mut backup = sample();
            backup.preferences.theme = legacy_theme.into();
            let restored = parse(&bytes(&backup)).unwrap();
            assert_eq!(restored.preferences.theme, fallback);
        }
        for motion in ["glide", "sweep", "bounce", "focus", "flicker", "step", "unroll"] {
            let mut backup=sample(); backup.preferences.motion=motion.into();
            let restored=parse(&bytes(&backup)).unwrap(); assert_eq!(backup.fingerprint().unwrap(),restored.fingerprint().unwrap()); assert_eq!(restored.preferences.motion,motion);
        }
        for shape in ["floating", "notch", "split"] {
            let mut backup = sample();
            backup.preferences.island_shape = shape.into();
            let restored = parse(&bytes(&backup)).unwrap();
            assert_eq!(backup.fingerprint().unwrap(), restored.fingerprint().unwrap());
            assert_eq!(restored.preferences.island_shape, shape);
        }
    }
    #[test] fn removed_page_backups_restore_to_applications_without_an_empty_panel() {
        let mut backup=sample();backup.preferences.island_page="compare".into();
        let restored=parse(&bytes(&backup)).unwrap();assert_eq!(restored.preferences.island_page,"applications");
        let mut ui=UiState::default();ui.island_expanded=true;ui.island_page="compare".into();ui.panel=Some("compare".into());
        restored.preferences.apply(&mut ui);assert_eq!(ui.island_page,"applications");assert!(ui.panel.is_none());
        assert!(!crate::state::valid_island_page("compare"));assert!(crate::state::valid_island_page("audio"));
        backup.preferences.island_page="unknown-page".into();assert!(parse(&bytes(&backup)).is_err());
        backup.preferences.island_page="audio".into();assert_eq!(parse(&bytes(&backup)).unwrap().preferences.island_page,"audio");
    }
    #[test] fn exporting_legacy_uncategorized_entries_normalizes_only_the_copy() {
        for category in ["", "deleted-category"] {
            let mut apps=sample().applications;apps.apps[0].category_id=category.into();
            let backup=ConfigurationBackup::capture(&apps,&UiState::default());
            let restored=parse(&bytes(&backup)).unwrap();assert_eq!(restored.applications.apps[0].category_id,"all");
            assert_eq!(apps.apps[0].category_id,category);
            assert_eq!(backup.fingerprint().unwrap(),restored.fingerprint().unwrap());
        }
    }
    #[test] fn rejects_corrupt_unknown_version_and_oversize() {
        assert!(parse(b"{broken").is_err()); let mut backup=sample();backup.version=99;assert!(parse(&bytes(&backup)).is_err()); assert!(parse(&vec![b' ';MAX_BYTES+1]).is_err());
    }
    #[test] fn rejects_duplicate_ids_and_missing_categories() {
        let mut backup=sample();backup.applications.apps.push(backup.applications.apps[0].clone());assert!(parse(&bytes(&backup)).is_err());
        let mut backup=sample();backup.applications.categories.push(backup.applications.categories[1].clone());assert!(parse(&bytes(&backup)).is_err());
        let mut backup=sample();backup.applications.apps[0].category_id="gone".into();assert!(parse(&bytes(&backup)).is_err());
    }
    #[test] fn rejects_invalid_preferences_and_secret_fields() {
        let mut backup=sample();backup.preferences.scale=0;assert!(parse(&bytes(&backup)).is_err());
        let mut value=serde_json::to_value(sample()).unwrap();value["apiKey"]="synthetic-test-secret".into();assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
        let mut value=serde_json::to_value(sample()).unwrap();value["applications"]["apps"][0]["password"]="synthetic".into();assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    #[test] fn restore_keeps_runtime_and_system_preferences() {
        let backup=sample();let mut ui=UiState::default();ui.paused=true;ui.wechat_enabled=true;ui.translation_pending=2;ui.settings_open=true;ui.island_appearance.hide_from_capture=true;ui.island_appearance.auto_check_updates=true;ui.island_appearance.avoid_fullscreen=false;
        backup.preferences.apply(&mut ui); assert!(ui.paused&&ui.wechat_enabled&&ui.settings_open&&ui.island_appearance.hide_from_capture&&ui.island_appearance.auto_check_updates);assert!(!ui.island_appearance.avoid_fullscreen);assert_eq!(ui.translation_pending,2);
        let text=String::from_utf8(bytes(&backup)).unwrap();for excluded in ["wechatEnabled","hideFromCapture","autoCheckUpdates","password","apiKey","clipboard"] { assert!(!text.contains(excluded)); }
    }
    #[test] fn confirmation_rejects_wrong_token_expiry_and_changed_configuration() {
        let current=sample();let mut pending=PendingConfiguration{token:"one".into(),backup:current.clone(),fingerprint:current.fingerprint().unwrap(),created:std::time::Instant::now()};
        assert!(pending.validate_confirmation("one",&current).is_ok());assert!(pending.validate_confirmation("cancelled",&current).is_err());
        let mut changed=current.clone();changed.applications.apps[0].name="changed".into();assert!(pending.validate_confirmation("one",&changed).is_err());
        pending.created=std::time::Instant::now()-std::time::Duration::from_secs(601);assert!(pending.validate_confirmation("one",&current).is_err());
    }
    #[test] fn preview_reports_replacements_order_and_preferences() {
        let before=sample();let mut after=before.clone();after.applications.apps[0].name="renamed".into();after.preferences.theme="light".into();
        let changes=describe(&before,&after);assert_eq!(changes.apps.changed,1);assert_eq!(changes.apps.removed,0);assert_eq!(changes.preferences.len(),1);
        after.applications.apps.clear();let changes=describe(&before,&after);assert_eq!(changes.apps.removed,1);assert_eq!(changes.apps.after,0);
    }
    #[test] fn missing_targets_are_preserved_and_network_references_are_not_probed() {
        let mut backup=sample();let original=backup.applications.apps[0].target.clone();
        let warnings=mark_targets(&mut backup,|_|false);assert_eq!(warnings.len(),1);assert_eq!(backup.applications.apps[0].target,original);assert_eq!(backup.applications.apps[0].broken,Some(true));
        backup.applications.apps[0].target="\\\\server\\share\\example.exe".into();mark_targets(&mut backup,|_|panic!("network path must not be probed"));
    }
    #[test] fn legacy_backups_without_telemetry_fields_default_cleanly_and_invalid_combinations_fail() {
        let mut value=serde_json::to_value(sample()).unwrap();
        let prefs=value["preferences"].as_object_mut().unwrap();
        prefs.remove("motion");prefs.remove("islandShape");prefs.remove("telemetryMode");prefs.remove("telemetryStyle");prefs.remove("telemetryNet");prefs.remove("telemetryCpu");prefs.remove("telemetryGpu");prefs.remove("telemetryMemory");prefs.remove("telemetryPeripherals");
        let restored=parse(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(restored.preferences.motion,"glide");
        assert_eq!(restored.preferences.island_shape,"floating");
        assert_eq!(restored.preferences.telemetry_mode,"smart");
        assert_eq!(restored.preferences.telemetry_style,"bar");
        assert!(restored.preferences.telemetry_net && restored.preferences.telemetry_cpu && restored.preferences.telemetry_gpu && restored.preferences.telemetry_memory && restored.preferences.telemetry_peripherals);

        let mut bad_shape = sample();
        bad_shape.preferences.island_shape = "island-unknown".into();
        assert!(parse(&bytes(&bad_shape)).is_err());

        let mut bad_motion=sample();
        bad_motion.preferences.motion="warp".into();
        assert!(parse(&bytes(&bad_motion)).is_err());

        let mut custom=sample();
        custom.preferences.telemetry_mode="always".into();
        custom.preferences.telemetry_net=true;
        custom.preferences.telemetry_cpu=false;
        custom.preferences.telemetry_gpu=false;
        custom.preferences.telemetry_memory=false;
        let round=parse(&bytes(&custom)).unwrap();
        assert_eq!(round.preferences.telemetry_mode,"always");
        assert!(round.preferences.telemetry_net && !round.preferences.telemetry_cpu && !round.preferences.telemetry_gpu && !round.preferences.telemetry_memory);

        let mut empty_metrics=custom.clone();
        empty_metrics.preferences.telemetry_net=false;
        assert!(parse(&bytes(&empty_metrics)).is_err());

        empty_metrics.preferences.telemetry_mode="off".into();
        assert!(parse(&bytes(&empty_metrics)).is_ok());

        let mut bad_mode=sample();
        bad_mode.preferences.telemetry_mode="floating".into();
        assert!(parse(&bytes(&bad_mode)).is_err());
    }
}
