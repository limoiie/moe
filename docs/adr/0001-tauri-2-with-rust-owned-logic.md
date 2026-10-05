# Tauri 2 承载界面，Rust 拥有全部逻辑

我们需要跨 macOS/Linux 的精致悬浮面板、键盘优先交互，且在中文环境下 IME（输入法）必须开箱可用——纯 Rust GUI（egui/Iced/Slint）的 IME 支持是硬伤，双份原生实现违背"主要使用 Rust"。决定采用 Tauri 2：Webview 只做视图层，Rust 侧承载热键、选区读写、扩展执行、AI、存储等一切逻辑。启动速度靠应用常驻菜单栏解决，不追求冷启动极致。前端是一层刻意做薄的 TS。
