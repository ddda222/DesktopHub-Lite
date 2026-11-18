use std::time::{Duration, Instant};
use tao::{dpi::LogicalSize, event::{Event, WindowEvent}, event_loop::{ControlFlow, EventLoopBuilder}, window::WindowBuilder};
use wry::{WebViewBuilder, WebViewBuilderExtWindows};

enum Probe { HideBriefly, Finish }

fn main() -> wry::Result<()> {
    let url = "http://127.0.0.1:4317/motion-study.html";
    let event_loop = EventLoopBuilder::<Probe>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new().with_title("Hub Lite · 隔离 WebView2 动效验收")
        .with_inner_size(LogicalSize::new(960, 750)).build(&event_loop).expect("测试窗口创建失败");
    let webview = WebViewBuilder::new().with_url(url)
        .with_additional_browser_args("--remote-debugging-port=0 --remote-debugging-address=127.0.0.1")
        .with_navigation_handler(move |target| target == url || target == "about:blank")
        .with_initialization_script("window.__hubMotionQA = true")
        .with_ipc_handler(move |request| {
            match request.body().as_str() {
                "hide-briefly" => { let _ = proxy.send_event(Probe::HideBriefly); },
                "finish" => { let _ = proxy.send_event(Probe::Finish); },
                _ => {}
            }
        }).build(&window)?;
    let mut resume = None;
    let expiry = Instant::now() + Duration::from_secs(210);
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::WaitUntil(resume.unwrap_or(expiry));
        match event {
            Event::UserEvent(Probe::HideBriefly) => {
                let _ = webview.evaluate_script("window.__nativeBefore = {...window.__probe, time:performance.now(), hidden:document.hidden}");
                let _ = webview.set_visible(false);
                window.set_visible(false);
                resume = Some(Instant::now() + Duration::from_millis(1000));
                *flow = ControlFlow::WaitUntil(resume.unwrap());
            },
            Event::UserEvent(Probe::Finish) | Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *flow = ControlFlow::Exit,
            _ if Instant::now() >= expiry => *flow = ControlFlow::Exit,
            _ if resume.is_some_and(|when| Instant::now() >= when) => {
                let _ = webview.evaluate_script("window.__nativeAfter = {...window.__probe, time:performance.now(), hidden:document.hidden}");
                window.set_visible(true);
                let _ = webview.set_visible(true);
                resume = None;
                *flow = ControlFlow::WaitUntil(expiry);
            },
            _ => {}
        }
    });
}
