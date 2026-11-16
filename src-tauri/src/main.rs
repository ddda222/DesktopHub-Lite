// Windows 桌面程序入口。
//
// 始终使用 windows 子系统：debug 构建也不能弹控制台窗口，
// 否则桌面上会多出一块带标题栏的黑框，既难看又挡住桌面点击。
// 诊断信息写入数据目录里的日志文件（见 logging.rs），不依赖 stderr。
#![windows_subsystem = "windows"]

fn main() {
    desktophub_lite_lib::run()
}
