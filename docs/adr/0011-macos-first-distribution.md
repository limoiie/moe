# macOS 首发形态：未签名 dmg + 菜单栏常驻 + 开机自启（可选）

进入「能给别人用」的阶段，需要把分发与常驻形态固定下来。

## 决定

- **分发**：`cargo tauri build` 产 `.app` + `.dmg`；GitHub Actions 的 Release 工作流（`workflow_dispatch` 或 `v*` 标签）上传工件。**不做签名与公证**：需要付费开发者账号与稳定的发布流程，当前阶段收益低。代价是首次打开要走「右键 → 打开」绕过 Gatekeeper，README「安装」一节写明。
- **常驻**：菜单栏 tray 是鼠标入口，也是键盘不可用时的兜底：显示面板 / AI 对话 / 开机自启 / 打开配置文件 / 退出 Moe。macOS 上应用以 `Accessory` 策略运行（无 Dock 图标、不参与 ⌘-Tab），与 ADR-0008 的浮层语义一致。
- **开机自启**：默认关，tray 里一个勾选项（`tauri-plugin-autostart` 写 LaunchAgent），用户显式开启才落盘；Moe 不在首次启动时替用户做这个决定。
- **平台顺序**：先把 macOS 打磨到 ready（功能、分发、文档），Linux 的实机验收与打包另票延后（见 `IIE4AD-363`）。Windows 维持 cfg 桩。

## 理由

- 未签名 dmg 已足够让本人与少数早期用户跑起来；签名/公证属于发布基建，等产品形态稳定后再补。
- tray 承担三件事：非键盘入口、权限/自启这类系统级开关、退出应用的唯一显式路径（Accessory 应用没有 Dock 菜单）。

## 代价

- 未签名带来一次性的 Gatekeeper 摩擦（写进了 README）。
- 开机自启依赖 LaunchAgent 路径，沙箱化分发（App Store）不适用——与 ADR-0003 的「编译内置扩展」一致，本产品不走 App Store。
