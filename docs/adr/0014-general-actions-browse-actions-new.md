# 三个通用动作由平台定键位、由 Extension 声明入口

面板、Side View、以及将来的任何子应用，都会各自长出自己的「记录列表」「动作清单」「新建」。
如果每个界面各绑一套键（历史用 ⌘B、动作藏在图标按钮里、新建用 ⌘N……），用户每进一个子应用就要重学一次——
这正是 Keyboard-first 产品最不该有的税。

Raycast 的做法值得照搬：**浏览（Browse）· 动作（Actions）· 新建（New）是平台级语义**，
键位固定为 ⌘P / ⌘⇧P / ⌘N，所有 Command 与子应用共用；具体「记录」是什么由扩展自己决定。

## 决定

平台侧（`moe-core`）：

- `SystemKey` 增 `Browse`（默认 ⌘P）与 `New`（默认 ⌘N）；`ShowAllActions` 同时接受 ⌘K（面板惯例）
  与 ⌘⇧P（子应用通用键），键位表里是同一行，展示串写 `⌘K / ⌘⇧P`。
- `Extension` 增两个可选入口：`browse_command()` / `new_command()`，默认 `None`。
  声明的 id 必须在 `commands()` 里（否则 `Registry::find` 路由不到，有守卫测试）。
- `Registry::entry_command(from_command, kind)` 按「当前命令所属 Extension」换出入口命令，
  经 IPC `entry_command` 交给 UI；UI 再照常 `invoke_command`。
  未声明 → UI 给一次内联提示（「该扩展没有记录列表」），不静默。
- 三个动作**只认 ⌘**（不认 ⌃）：⌃P/⌃N 已被 Navigation 占用，本轮不做平台级修饰键重映射。

UI 侧：`ui/src/keymap.ts` 是这三个语义的 UI 镜像（`generalActionOf(event)`），
命令盘与 Side View 都用它识别，再各自决定落点：

| 语义 | 命令盘（`ui/src/main.ts`） | Side View（`ui/src/chat.ts`） |
|---|---|---|
| Browse ⌘P | 调 `entry_command(browse)` 进该扩展的记录列表 | 弹出历史会话悬浮卡 |
| Actions ⌘⇧P | 打开 Focused Item 的动作层（同 ⌘K） | 打开更多操作菜单 |
| New ⌘N | 调 `entry_command(new)` 新建记录 | 新对话 |

AI 扩展声明：Browse → `ai.search-history`（历史会话），New → `ai.quick-ask`（面板里的会话页：
一问一会话，下一条提问自然是新会话）。

## 代价

- 没有声明入口的扩展按 ⌘P/⌘N 只得到一句提示；这是有意的——「没有记录」是扩展的事实，
  平台不该替它编一个。
- ⌘N/⌘P 覆盖了系统的「新窗口/打印」，但只在我们自己的非激活面板与侧栏窗口内生效，不劫持宿主应用。
- 未来若要在 Linux 上把 ⌃ 当主修饰键，需要平台级的一次统一重映射，而不是在这三处各打补丁。

## 修订：Actions 全平台统一 ⌘K；Materialize 改绑 ⌘J

「一个语义两个键」（面板 ⌘K / 子应用 ⌘⇧P）在实践中是负担：Side View 的用户要额外记一个 ⌘⇧P，
而 ⌘K 在 Side View 里空着。现合并为**全平台 ⌘K**——命令盘与 Side View 用同一个键打开同一张
actions card（ADR-0028 的「一张卡两个窗口」连键位也统一）：键位表展示串由 `⌘K / ⌘⇧P` 收敛为
`⌘K`，`generalActionOf` 中 ⌘K（无 shift）即 actions、⌘P（无 shift）即 browse，⌘⇧P 不再绑定。
Materialize 由 ⌘M 改绑 **⌘J**（应用户要求；⌘M 亦与 macOS minimize 的肌肉记忆冲突）。AI 扩展
answer item 声明的 keybinding、各处提示文案与 README 键位表同步更新；keymap 表仍是唯一事实源，
Kbd 与 tooltip 不会漂移。

## 修订：AI 的 New 归并到 Quick Ask，并为两个界面加直达键（MOE-0016）

AI 扩展原先声明 New → `ai.new-chat`，与 `ai.quick-ask` 是同一个动作（都返回空白会话页），
命令盘里却多一条重复行。现删除 `ai.new-chat`：`new_command()` 直接解析到 `ai.quick-ask`，
各页面上的 ⌘N 行为不变（仍是打开空白会话页）；「New Chat」只留在 UI 的固定文案里
（actions card 的 General 行、Side View 表头按钮）。两个界面的直达键见 ADR-0036 修订：
⌘/ = 面板内 Quick Ask，⌘⇧/ = 直开侧栏聊天（`ai.side-chat`）。
