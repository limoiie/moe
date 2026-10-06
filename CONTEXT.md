# Moe

一个键盘优先（keyboard-first）的桌面命令盘：用一套统一的快捷键语义驱动所有能力，让用户在任意应用中选中文字后呼出面板、执行命令并把结果回写到光标处。

> 本文件只是词汇表（glossary）。实现细节一律不进这里，见 `docs/adr/`。

## Language

### 能力结构

**Extension（扩展）**:
能力的组织单元：聚合若干 Command，并独占自己的存储与历史。AI 问答就是一个 Extension。
_Avoid_: 插件、plugin、子应用（口语可用，正式一律用 Extension）

**Command（命令）**:
Extension 暴露给用户的一个可调用入口，均可在命令盘中被搜到；是用户与 Extension 内部数据交互的接口。
_Avoid_: 子命令、快捷指令、action

**Namespace（命名空间）**:
每个 Extension 独占的存储隔离域；Extension 的历史与数据只存在于自己的 Namespace 中，跨 Extension 不可见。
_Avoid_: 沙箱、分区

**Side View（侧栏视图）**:
Extension 可声明的一种常驻呈现形态，区别于平台默认的悬浮面板；悬浮面板中的会话可"实体化"为 Side View。每个 Side View 属于且仅属于一个 Extension。
_Avoid_: 侧边栏模式、sidebar（正式术语用 Side View）

### 面板与交互

**Command Panel（命令盘）**:
由全局快捷键呼出的居中悬浮面板，是用户与所有 Command 相遇的唯一场所。
_Avoid_: 启动器、launcher、悬浮框（口语可用）

**Summon Key（呼出键）**:
唤出 Command Panel 的唯一键位；默认双击 ⌘，可配置为其他修饰键双击或组合键（ADR-0008）。未获系统授权时面板内给出引导。
_Avoid_: 快捷键（那泛指 Keymap 内所有键）

**Input Bar（输入栏）**:
Command Panel 顶部的统一输入区，永远处于可输入状态（像浏览器地址栏的位置，但语义是输入而非路径显示）。所有 Extension 形态中它的位置与行为一致。
_Avoid_: 搜索框、地址栏

**Result List（结果列表）**:
Input Bar 之下随输入实时更新的候选列表；其中当前被选中的一个称为 Focused Item。
_Avoid_: 下拉列表

**Section（来源分组）**:
命令盘根页按「来源」分组显示：每个 Extension 一个 section，组头是扩展名，
组序按组内最优项（ADR-0020）。组头不可聚焦、不参与键盘导航。将来新来源（如文件搜索）就是新扩展的新 section。
_Avoid_: 分类、类别

**Page Shape（页面形态）**:
结果页只有三种形态：**列表**（list，单列）、**两栏**（split，左列表 + 右详情）、
**详情**（detail，唯一一条结果整屏）。由平台在一处判定与渲染（`ui/src/layout.ts`），
连同分栏比例与面板窗口尺寸都是全局统一；Extension 只提供内容（Item 可带 `detail`），不自己排版（ADR-0018）。
_Avoid_: 布局模板、视图模式

**Live List（实时列表）**:
Command 的一种列表语义（`CommandMeta.live`）：进入该 Command 后，Input Bar 的每次变化
都会用新查询重跑本 Command（如「AI: 搜索历史会话」随输入筛标题）。非 Live 的 Command
中输入变化仍是命令盘检索。
_Avoid_: 动态搜索、自动补全

**Focused Item（焦点项）**:
Result List 中当前接受键盘操作的唯一 item。所有主/副操作都作用于它。
_Avoid_: 高亮项、选中项（"选中"保留给其他应用的文字选区）

**Detail Full（详情整屏）**:
结果层的一种形态：唯一一条结果本身即内容（AI 回答、系统通知），详情占满面板；
与之相对的是左列表 + 右详情的列表形态。由 Command 的结果声明（`detailFull`），
不由 UI 按条数猜（ADR-0013）。
_Avoid_: 全屏卡片、大图模式

**Apply（应用）**:
对 Focused Item 的主操作（Enter）：执行该 item 所代表动作的默认语义。
_Avoid_: 执行、打开、启用

**Stop（停止生成）**:
流式产出中的 Item 标记为 `pending`；此时 Esc 的第一优先级是请求停止（平台级，一次停止全部进行中的生成），
保留已生成部分而不是回滚（ADR-0006 增补）。
_Avoid_: 取消、中断

**Secondary Action（副操作）**:
对同一 Focused Item 的具名替代操作，各有固定快捷键（如 ⌥Enter 复制纯文本、复制 HTML 等）。主副之分是 Command 的一致语义约定，不由各 Extension 自定。
_Avoid_: 右键菜单

**Copy（复制）**:
SecondaryCopy（默认 ⌥Enter）的默认语义：把 Focused Item 的文本写入系统剪贴板，**不动宿主应用**（与 Write Back 相对，ADR-0002 增补）。
_Avoid_: 剪贴、拷贝到剪贴板

**Selection（选区）**:
指用户在其他应用中选中的内容：文字选区，或 Finder 里选中的文件（ADR-0021）。
Moe 自身的列表选择不叫 Selection，叫 Focus。
呼出面板前抓到的选区会随提问自动成为 AI 的上下文；选中的文件会成为提问附件。
_Avoid_: 用 Selection 指代 Focused Item

**Keymap（统一键位表）**:
跨所有 Extension 完全一致的平台级键位语义（导航、Apply、副操作、Esc 分层回退、展开全部动作）。Extension 不得覆写，只能为自己的 Command 注册具名动作（动作可附带快捷键）。
_Avoid_: 快捷映射、绑定

**Show All Actions（展开全部动作）**:
⌘K（子应用通用键位：⌘⇧P）触发的层级行为：把当前 Focused Item 的全部主/副操作列为一层列表供键盘选择。
一个语义两个键位：⌘K 是命令盘惯例，⌘⇧P 供子应用复用（ADR-0014）。
_Avoid_: 命令菜单

**Browse（浏览记录）**:
平台通用动作，默认 ⌘P：打开当前 Extension 的记录列表（AI = 历史会话）。
入口由 Extension 声明（`browse_command`）；没声明就是没有记录可浏览，平台只给一次内联提示（ADR-0014）。
_Avoid_: 历史、列表视图（Browse 特指这个键位语义）

**New（新建记录）**:
平台通用动作，默认 ⌘N：新建一条记录（AI = 新会话）。入口同样由 Extension 声明（`new_command`）；
与 Browse、Actions 一起构成跨 Command/子应用的三个通用动作（ADR-0014）。
_Avoid_: 新建窗口、加号

**Action Bar（底部动作条）**:
面板右下角悬浮的两个按钮：**主操作**（当前 Focused Item 的主操作，生成中变为「停止生成」）
与**动作**（`⌘K` / `⌘⇧P`，展开全部动作）。浮在 Result List 之上（带阴影与模糊），
取代过去的 Hints Bar：键位与动作统一收进这一处，不再常显整行提示（ADR-0015）。
_Avoid_: 提示条、Hints Bar（旧名）

**Write Back（回写）**:
把命令结果交付回用户文本上下文的方式：有 Selection 则替换之，无则插入到光标处。只有回写类 Command 才有此行为。
_Avoid_: 粘贴（粘贴只是回写的一种实现手段）

### AI 问答

**Conversation（会话）**:
AI 问答 Extension 内的一次连续问答记录，归属其 Namespace。
_Avoid_: 聊天、chat

**Quick Ask（快捷提问）**:
直接在 Command Panel 内发起的单轮/短轮提问，结果可回写。
_Avoid_: 快速模式

**Materialize（实体化）**:
把 Command Panel 中的当前会话转为该 Extension 的 Side View 的动作。
_Avoid_: 展开、弹出

**Attachment（附件）**:
提问时随消息携带的本地文件（v1：文本与图片）。在输入栏以 `@path` mention 表达（ADR-0010）；
内容在请求时现读（文本内联、图片进多模态），历史里只存引用。
Finder 里选中的文件同样成为附件（ADR-0021）；纯附件无正文时面板提示写问题、侧栏换通用请求句，不把原始路径当问题发出。
_Avoid_: 上传、文件对象、upload
