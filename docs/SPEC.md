# Quire 项目规格（原始需求文档）

> 本文档是项目的初始需求与长期约束说明，随项目保留存档。README 中为其浓缩版。

---

你现在要从零开始开发一个现代、轻量、GPU 加速的 Notion-like 本地文档应用。

这是一个长期项目，不要把它理解成一次性的 Demo。第一目标平台是 Windows，后续再考虑 Android。项目核心要求是：

1. 界面必须好看，达到现代桌面生产力软件的视觉质量，参考 Notion、Linear、Obsidian、Craft、Raycast、Arc 等产品的设计理念，但不要直接复制任何品牌 UI、Logo、图标或视觉资产。
2. Windows 下必须尽量低内存、低 CPU。
3. UI 必须使用 GPU 加速渲染。
4. 不使用 Electron、Chromium、WebView2、Tauri、React、Vue、Web 技术作为主要 UI 运行时。
5. 使用 Rust + Slint。
6. UI 与核心业务逻辑严格分离，但默认保持单进程，不要把 Frontend 和 Backend 拆成两个进程。
7. 代码必须非常适合 Coding Agent 持续修改和扩展，避免形成像输入法/TSF 那种平台耦合极深、修改成本极高的架构。
8. 第一版不要做同步、协作、云端、插件市场、AI、实时多人编辑等大型能力。
9. 优先把“漂亮的桌面 UI + Block Editor + 本地存储 + 极低资源占用”做完整。
10. 所有架构选择都要优先考虑长期可维护性，而不是为了短期功能量堆复杂度。

==================================================
一、项目技术底座
========

不要直接 fork 一个完整的第三方 Notion-like 项目。

以官方 Slint Rust Template 为工程起点：

https://github.com/slint-ui/slint-rust-template

这个仓库只作为工程脚手架，不要把里面的 Todo 示例逻辑继续发展成最终架构。

参考以下官方仓库，但只学习架构和 UI 写法：

1. Slint 主仓库：
   https://github.com/slint-ui/slint

2. 官方 cargo-ui：
   https://github.com/slint-ui/cargo-ui

3. 官方 node-editor-cpp：
   https://github.com/slint-ui/node-editor-cpp

特别参考 node-editor-cpp 对以下问题的处理方式：

* `.slint` 负责视觉、布局、交互
* backend 负责 model
* UI callback 与 backend API 有明确边界
* 动态模型通过 Slint model 接入
* 复杂交互状态不要散落在 UI 各处

不要复制它的具体项目结构，只吸收架构思想。

当前 Slint 版本优先使用 1.18.x 或当前稳定的 1.x 小版本，不要无理由锁死过旧版本。2026-09-16 发布的 Slint 1.18 已经包含更好的大文本渲染/编辑性能、FlexboxLayout、运行时 z-order、拖拽、改进的代码生成等功能，这些能力对本项目有直接价值。

==================================================
二、项目总体架构
========

不要采用：

UI → JS → API → DB

不要采用：

UI → WebView → Web Backend

不要采用：

多个进程通过 IPC 通信

推荐：

```
                 Application
                      |
        ┌─────────────┴─────────────┐
     Slint UI Layer              Rust Core
        |                             |
   ┌────┼────┐            ┌──────────┼──────────┐
Sidebar Editor Command   Document/Page/Block   Model → DB   Search
   |                        Services
   |                     Import / Export
```

整个应用默认保持单进程。

UI 层：

* `.slint`
* 视觉
* 布局
* 动画
* 交互
* 当前 UI 状态
* Focus
* Selection
* Popup
* Menu

Rust Core：

* Document Model
* Page Model
* Block Model
* Undo/Redo
* Persistence
* SQLite
* Search
* Import/Export
* Application state
* Command dispatch
* 后台任务

原则：

UI 不直接碰数据库。

UI 不直接执行磁盘 IO。

UI 不自己管理业务数据的一致性。

Rust Core 不负责视觉细节。

Rust Core 不应该知道具体某个按钮是什么颜色。

==================================================
三、目录结构
======

从第一天就建立清晰目录。

推荐：

src/
main.rs

```
core/
    mod.rs
    document.rs
    page.rs
    block.rs
    inline.rs
    command.rs
    history.rs
    selection.rs

storage/
    mod.rs
    database.rs
    migrations.rs
    repository.rs

services/
    mod.rs
    document_service.rs
    search_service.rs
    import_service.rs
    export_service.rs

platform/
    mod.rs
    windows.rs

app/
    mod.rs
    state.rs
    controller.rs
```

ui/
AppWindow.slint
Theme.slint
Colors.slint
Typography.slint
Icons.slint

```
components/
    AppShell.slint
    Sidebar.slint
    SidebarItem.slint
    PageTree.slint
    Editor.slint
    EditorBlock.slint
    BlockHandle.slint
    BlockMenu.slint
    SlashMenu.slint
    CommandPalette.slint
    SearchPanel.slint
    TopBar.slint
    Button.slint
    IconButton.slint
    Tooltip.slint
    ContextMenu.slint
    IconPicker.slint
    Dialog.slint

blocks/
    ParagraphBlock.slint
    HeadingBlock.slint
    TodoBlock.slint
    BulletBlock.slint
    NumberedListBlock.slint
    QuoteBlock.slint
    CodeBlock.slint
    DividerBlock.slint
```

tests/
fixtures/
integration/

benchmarks/
documents/
scripts/

docs/
ARCHITECTURE.md
UI_ARCHITECTURE.md
EDITOR_ARCHITECTURE.md
PERFORMANCE.md
ROADMAP.md
DECISIONS.md

PLAN.md
README.md

不要把所有 UI 写进一个 `main.slint`。

不要把所有 Rust 逻辑写进 `main.rs`。

==================================================
四、第一阶段：工程初始化
============

目标：

M0：能够稳定编译、启动，并确认 GPU renderer。

任务：

1. 从官方 slint-rust-template 初始化项目。
2. 替换项目名称。
3. 使用当前稳定 Slint 1.x。
4. Windows 使用 MSVC。
5. 建立 Debug / Release 两种构建。
6. 配置 Slint LSP。
7. 确认 VS Code 能正常获得：

   * 语法高亮
   * 补全
   * 跳转
   * live preview
8. 建立最基本的 CI：

   * cargo check
   * cargo test
   * cargo build --release
9. 创建最小主窗口。
10. 创建 GPU renderer 验证 Demo。

不要一开始就堆各种依赖。

尤其不要因为“以后可能需要”提前安装几十个 crate。

每个依赖都必须回答：

* 为什么需要？
* 是否能够用标准库解决？
* 是否会增加 runtime memory？
* 是否会增加后台线程？
* 是否会增加 build complexity？

==================================================
五、GPU Renderer 选择
=================

必须实际验证 renderer，不允许只根据网上印象决定。

Slint 当前 Winit backend 可以选择：

* FemtoVG
* FemtoVG + WGPU
* Skia
* Skia OpenGL
* Software

Windows 项目重点测试：

A. winit-femtovg-wgpu

B. winit-skia

必要时额外测试实验性的 Vello renderer，但不要在第一版直接使用实验 renderer。

优先关注：

1. 文本清晰度
2. 滚动流畅度
3. CPU
4. GPU
5. RAM
6. 首次启动时间
7. 空闲时 CPU 是否接近 0
8. 是否存在持续 frame
9. 长文本表现

注意：

GPU 加速不是目标本身。

目标是：

“用 GPU 渲染 UI，同时避免因为无意义持续重绘导致 CPU 很高。”

如果静止界面一直有动画、timer 或 render loop，必须查清楚原因。

==================================================
六、性能基线
======

在真正开发 UI 之前，建立 benchmark。

必须至少记录：

1. Startup time
2. Idle Working Set
3. Idle Private Bytes
4. Idle CPU
5. Simple document RAM
6. 1000 Block document RAM
7. 5000 Block document RAM
8. 10000 Block document RAM
9. Scroll CPU
10. Typing CPU
11. Search CPU
12. Page switching latency

必须区分：

Debug

和：

Release

所有正式 benchmark 以 Release 为准。

不要使用一个“看起来很漂亮”的页面作为唯一 benchmark。

建立四种固定场景：

Benchmark A：

空白窗口，只包含 App Shell。

Benchmark B：

100 个简单 Paragraph Block。

Benchmark C：

5000 个简单 Paragraph Block。

Benchmark D：

10000 个简单 Paragraph Block。

Benchmark E：

持续输入文本。

Benchmark F：

连续滚动。

Benchmark G：

快速切换 100 个页面。

每次关键架构修改后重新测试。

把结果写入：

docs/PERFORMANCE.md

==================================================
七、第二阶段：视觉系统
===========

这一步非常重要。

不要先做数据库。

先做一个“看起来已经像产品”的桌面 UI。

目标：

即使所有数据都是 mock data，截图也应该具有商业软件的完成度。

视觉参考：

Notion：

* 极简
* 大量留白
* 文档感
* 清晰层级

Linear：

* 高信息密度
* 精细 hover
* 很好的快捷操作
* 克制动画

Obsidian：

* 桌面知识库结构
* Sidebar + Document

不要直接复制这些软件。

建立自己的视觉语言。

---

## 主题系统

创建：

Theme.slint

Colors.slint

Typography.slint

Icons.slint

统一定义：

* background
* surface
* surface-hover
* surface-selected
* text-primary
* text-secondary
* text-muted
* border
* accent
* danger
* success
* code-background

定义：

* 标题字号
* 正文字号
* Sidebar 字号
* Command Menu 字号
* 行高
* 字重
* 圆角
* spacing scale

例如：

spacing-xs
spacing-sm
spacing-md
spacing-lg
spacing-xl

不要在 100 个 `.slint` 文件里各自写：

`8px`

`9px`

`11px`

`13px`

然后最后无法统一修改。

---

## 颜色

默认设计至少：

Light

Dark

但不需要第一阶段支持用户自定义颜色。

浅色模式优先：

* 接近白色的主背景
* 非纯黑文字
* 极轻的 border
* 低对比度 sidebar

深色：

* 不使用纯黑
* 不使用大面积强对比
* 文本层级清晰

---

## 动画

动画原则：

“有反馈，但不持续运行。”

允许：

* Hover fade
* Sidebar selection transition
* Menu enter
* Command Palette enter
* Drag feedback
* Popup scale/fade

禁止：

* 无限 loop 动画
* 每帧更新 UI state
* 无意义的呼吸灯
* 背景动画

---

## 窗口

Windows 应用应该考虑：

* 自定义 title bar
* WindowMoveArea
* 最小化
* 最大化
* 关闭
* window resize
* system tray 后续可选

但第一版不要花大量时间做 Windows 原生窗口特效。

==================================================
八、第三阶段：App Shell
================

先做：

┌─────────────────────────────────────────┐
│             Top / Title Bar             │
├──────────┬──────────────────────────────┤
│          │                              │
│ Sidebar  │          Document            │
│          │                              │
│ Workspace│       Editor Area            │
│ Pages    │                              │
│ Search   │                              │
│ Settings │                              │
│          │                              │
└──────────┴──────────────────────────────┘

Sidebar：

* Workspace
* Favorites
* Recent
* Page Tree
* Search
* Settings

顶部：

* 页面标题
* breadcrumb 可选
* Search
* 更多菜单

不要第一版做得像传统 IDE。

应用整体应该像现代生产力软件。

---

## Sidebar 行为

要求：

* hover
* selected
* right click
* collapse
* expand
* drag
* context menu

Page Tree：

支持：

Page

Sub Page

无限层级理论上允许，但视觉上需要限制展开复杂度。

不要一开始制作 10 万节点测试。

==================================================
九、第四阶段：Document Model
=====================

这是 Rust Core 的核心。

不要：

Page = 巨大 String

推荐：

Document

包含：

Page

包含：

Block Tree

示例：

Page
├── Heading
├── Paragraph
├── Paragraph
├── Todo
├── BulletList
│    ├── Paragraph
│    └── Paragraph
├── Quote
└── Code

每个 Block：

id

parent_id

order

type

properties

children

不要使用数组 index 作为永久 ID。

ID 必须稳定。

推荐 UUID 或适合本项目的轻量 ID。

排序必须支持插入和移动。

---

## Block Type

第一期：

paragraph

heading_1

heading_2

heading_3

bullet

numbered

todo

quote

code

divider

后续再添加：

image

bookmark

callout

table

toggle

database

不要第一期做这些。

2026-09-20 更新：上面这批已排进 §三十七（媒体 / 结构 / 嵌入三批）与 §三十九（database），不再是「以后再说」，按 §二十九 的 M10–M14 执行。第一期（M0–M8）约束仍然有效，不得借这条提前动块模型。

==================================================
十、Inline Model
==============

Notion 的正文不是单纯字符串。

因此预留：

Inline Text

Inline Code

Bold

Italic

Link

Strike

Highlight

2026-09-21 更新：这批预留里除 Highlight 外都已在 M6 实现，另外多了一项 §三十七 批次 C 要的 Math（行内 `$…$` 与 `math` 块，ADR-0038）。这不回头改 M6 的验收：Math 是新增 mark kind，靠 `MarkKind::as_str` 落库，没有迁移。

但第一阶段不要一次把全部实现。

推荐：

Block

↓

Text Content

↓

Inline Span[]

每个 Span：

text

marks

link

style

---

## 重要限制

Slint 目前没有一个现成的完整 Notion-grade Rich Text Editor。

因此不要假定：

TextEdit = Notion Editor

不是这样的。

需要自己建立：

Editor Model

Selection

Caret

Block insertion

Block deletion

Block splitting

Block merging

Inline mark

Document command

==================================================
十一、第五阶段：Block Editor MVP
========================

这是第一阶段真正的核心。

必须先做稳定，而不是先做富文本。

必须实现：

1. 输入文字
2. Enter 新建 Block
3. Backspace 删除
4. Backspace 合并 Block
5. Delete
6. Up / Down
7. Left / Right
8. Home / End
9. Ctrl+A
10. Ctrl+C
11. Ctrl+V
12. Ctrl+Z
13. Ctrl+Shift+Z 或 Ctrl+Y
14. 鼠标选区
15. 鼠标点击定位
16. Todo
17. 切换 Block 类型
18. 删除 Block
19. 移动 Block

要求：

输入不能明显卡顿。

不要每输入一个字符就：

serialize document

write SQLite

rebuild all UI

refresh all block

正确做法：

用户输入

↓

Editor Controller

↓

更新当前 Block

↓

局部 UI 更新

↓

debounced persistence

---

## 中文输入

这是硬性要求。

禁止自己实现：

TSF

IME engine

Candidate Window

Composition Framework

输入法协议

Text Service

不要重复做你之前输入法项目的复杂基础设施。

优先使用 Slint 的文本输入设施和 Windows 系统 IME。

只有遇到 Slint/Windows 的实际 bug 时，再做最小的平台 adapter。

==================================================
十二、第六阶段：Block Rendering
=======================

Block Editor 必须考虑 Virtualization。

不要：

10000 blocks

↓

10000 个复杂永久 UI Item

↓

10000 个 TextEdit

正确目标：

viewport

↓

visible blocks

↓

nearby blocks

↓

复用/创建必要的 UI

核心原则：

不可见内容尽可能不占据昂贵的 UI 资源。

不要把每一个 Block 都永久绑定一个复杂 editor。

特别是 TextEdit。

因为：

TextEdit 是交互控件。

不是纯文本渲染器。

考虑将：

非编辑状态

和：

编辑状态

分开。

普通 Block：

轻量展示

当前编辑 Block：

进入 editor mode

例如：

普通：

ParagraphBlockView

当前：

ParagraphEditor

只有焦点 Block 使用真正 TextEdit。

这样可以极大降低长文档常驻 UI 成本。

==================================================
十三、第七阶段：Selection / Caret / Focus
=================================

这是编辑器稳定性的关键。

建立明确的 EditorState：

focused_block_id

selection_start

selection_end

anchor

cursor

composition_state

active_block

editing_mode

不要让每个 `.slint` 文件自己维护一套 selection。

Rust 中维护核心状态。

UI 只展示。

==================================================
十四、第八阶段：Command 系统
==================

所有核心编辑操作都应该尽量变成 Command。

例如：

InsertText

DeleteText

SplitBlock

MergeBlock

SetBlockType

MoveBlock

DeleteBlock

InsertBlock

ToggleTodo

ApplyMark

Undo

Redo

这样 Undo/Redo 就可以天然建立在 Command history 上。

不要实现：

“撤销的时候重新读取数据库”

Undo 应该针对内存中的 Document Model。

数据库只是 persistence。

==================================================
十五、第九阶段：Slash Command
=====================

实现 Notion 风格：

输入：

/

弹出：

┌──────────────────────┐
│ Search blocks...     │
├──────────────────────┤
│ Text                 │
│ Heading 1            │
│ Heading 2            │
│ Bullet List          │
│ Numbered List        │
│ Todo                 │
│ Quote                │
│ Code                 │
│ Divider              │
└──────────────────────┘

要求：

* 键盘操作
* 上下选择
* Enter 确认
* Esc 取消
* 模糊搜索
* 自动定位
* 不抢正常输入

Slash menu 不应该在 QML/Slint 中硬编码大量业务判断。

由 Rust 返回 command descriptors。

UI 负责展示。

==================================================
十六、第十阶段：Command Palette
=======================

提供全局：

Ctrl+K

或你最终确定的快捷键。

支持：

Open Page

Create Page

Search

Change Theme

Toggle Sidebar

Go Back

Go Forward

等等。

Command Palette 与 Slash Menu 必须复用底层 command registry。

不要写两套。

==================================================
十七、第十一阶段：Page Tree
==================

实现：

Create page

Rename page

Delete page

Duplicate page

Create subpage

Move page

Collapse

Expand

Search page

Favorites

Recent

页面树操作应该是异步但不应阻塞 UI。

大型 Tree 必须考虑 virtualization。

不要一次把整个 workspace 渲染成巨大的 UI tree。

---

本阶段只管树的结构。页面自身的图标、封面、版式、锁定、版本历史、模板见 §三十八；反向链接与提及见 §四十。

==================================================
十八、第十二阶段：SQLite
===============

使用 SQLite。

数据库至少考虑：

workspaces

pages

blocks

block_children

metadata

settings

后续：

search index

attachments

不要一开始引入：

Postgres

server

cloud database

Redis

sync backend

所有数据优先本地。

---

## 事务

以下操作应该具备事务一致性：

Create Page

Delete Page

Move Page

Move Block

Merge Block

Split Block

Bulk Update

Undo/Redo persistence

不要每个 SQL 单独提交然后让中间状态暴露。

==================================================
十九、第十三阶段：自动保存
=============

自动保存策略：

内存修改

↓

dirty

↓

debounce

↓

batch persistence

例如用户连续输入时：

不要：

每字符：

UPDATE SQLite

而应该：

停止输入一小段时间

↓

批量保存

特殊情况下：

Ctrl+S

立即保存。

程序关闭：

尽可能 flush dirty state。

==================================================
二十、第十四阶段：Search
===============

第一版实现：

Page title search

Content search

Block text search

不要做复杂 AI search。

搜索必须：

异步

不阻塞 UI

结果实时显示

支持 keyboard navigation。

后续可以升级：

SQLite FTS

但第一版先留接口。

页内查找（Ctrl+F，M7）是这一阶段的另一条，只查已经打开的那一页。它有一条画法规格：
**计数条说 2 / 16，页面上就要看得见 16 个命中**。一个命中 = 一个 word-run 单元格
（ADR-0041 的切分，ADR-0043 把它当成多出来的两个字节边界），命中的边界落在词中间就把词切开，
落在标记里就把整段标记染上——标记那格永不可切。底色只负责说「这一格」，标记由边框承担：
没有任何一种底色能同时做到「自己在白纸上达到 3:1」和「不压垮九种块文字色」，
所以量出来的是 #ffe9a8 + #bd6408（浅色）与 #3d3413 + #a87718（深色）。
命中在表格单元或分栏盒子里时没有自己的行，只能搭画它的那一行（ADR-0028）。
「看得见」不等于「都是框」：抽样张那一页 16 个命中画出 14 个，剩下 2 个在被光标占住的那一块里，
那里画的是编辑器自己的选中态而不是单元格——计数与画面必须能这样对上账，对不上就是漏画。
引用块和高亮块各只有一个自己的 `Text`，所以它们一度什么框都不画，而计数条照报 16；
现在凡是画正文的地方都走同一套 runs，各带自己的边框（引用块的缩进、高亮块让开 emoji 的那 40px）。

==================================================
二十一、第十五阶段：视觉质量迭代
================

到这个阶段，必须暂停功能开发，专门做视觉。

要求 Agent 自己审查：

* spacing
* alignment
* typography
* hierarchy
* hover
* active
* selected
* focus
* popup
* menu
* sidebar
* scrollbar
* empty state
* loading state
* error state

不要出现：

一个按钮 8px 圆角

另一个 6px

另一个 12px

这种没有系统的设计。

不要默认使用过多阴影。

不要让每个区域都有 border。

要让 UI 通过：

字体

间距

颜色

层级

来形成视觉结构。

==================================================
二十二、第十六阶段：性能优化
==============

这一阶段不是：

“把 UI 做丑来省内存”。

性能优化必须优先来自结构。

重点检查：

1. QML/Slint Item 数量
2. TextEdit 数量
3. Text layout 次数
4. 图片缓存
5. string clone
6. model 更新范围
7. database write frequency
8. timer 数量
9. animation 数量
10. redraw frequency

重点目标：

静止页面：

尽量没有持续 CPU 工作。

编辑：

只更新受影响 Block。

滚动：

只处理必要内容。

搜索：

后台执行。

保存：

debounced。

图片：

lazy load。

大型文档：

virtualized。

---

## 长文档测试

必须有：

1000 block

5000 block

10000 block

分别测试：

打开

滚动

输入

删除

移动

搜索

切页

退出

不得只在 100 个 Block 下判断性能。

==================================================
二十三、第十七阶段：内存诊断
==============

如果 RAM 高，不能简单说：

“Slint 就是这么高。”

必须定位：

Rust heap

Slint runtime

text cache

image cache

model

document

database

OS graphics memory

分别分析。

尤其注意：

Task Manager 中：

Memory

GPU Memory

Private Working Set

可能不是同一个东西。

记录时至少区分：

CPU-side memory

GPU-side memory

shared GPU memory

不要把 GPU 显存直接当成普通 RAM。

==================================================
二十四、第十八阶段：Release Packaging
===========================

Windows Release：

* release profile
* LTO
* strip/symbol strategy
* panic strategy 根据需求决定
* debug artifacts 分离

但不要为了让 exe 数字看起来小，而牺牲启动性能或者可维护性。

优先优化：

runtime memory

runtime CPU

startup

而不是：

exe 文件大小

==================================================
二十五、第十九阶段：Crash Recovery
========================

必须考虑：

突然退出

崩溃

断电

数据库写入中断

不能因为最后 1 秒没保存导致整个页面损坏。

至少实现：

atomic transaction

safe migration

backup / journal 策略

启动时 database integrity check。

==================================================
二十六、第二十阶段：Import / Export
=========================

第一版：

Markdown import

Markdown export

Plain text

后续：

HTML

JSON

不要求：

Notion API

Word

PDF

第一版不要做。

一条通道规则在 2026-09-22 被 §三十八 的模板测试抓到并补上（ADR-0049）：**一个没有正文的块仍然要写出它的标记** —— 空的列表项写 `-`（编号项写 `1.`，且不带尾随空格），空的引用写 `>`，空的标题写 `#`。在此之前导出会把空列表项整行丢掉，于是「往返」这件事在空行上是假的：一份模板导出再导入就少一行。这条对普通页面导出同样成立，而且对模板尤其重要——一个空的 bullet 就是模板的内容，它是留给别人填的那一行。钉住它的是 `an_empty_block_exports_its_bare_marker_and_comes_back_as_itself`，五种能为空的块各走一遍。

==================================================
二十七、第二十一阶段：Windows UX
=====================

Windows MVP 稳定之后再增加：

* native context menu
* file association
* system tray
* global-ish shortcut 中可行的部分
* startup options
* drag/drop files
* clipboard rich content —— 文字部分已由 ADR-0025 的富文本粘贴交付，位图部分 2026-09-21 交付（ADR-0035，见 §三十七 批次 A）；HTML/RTF 等其它富格式仍未做

Slint 1.17 已经加入 drag and drop、system tray、tooltips、model row two-way bindings，可以优先利用这些现成能力，而不是自行造轮子。

==================================================
二十八、第二十二阶段：Android
==================

Windows 稳定以后才开始。

共享：

Document Model

SQLite schema

Command

Undo/Redo

Search

Import

Export

尽可能共享：

Slint UI components

但不要把 Windows UI 强行缩成手机 UI。

Android 需要：

底部/侧边导航重新设计

触摸交互

虚拟键盘

编辑器移动优化

safe area

touch target

但核心模型保持相同。

注意：Slint 当前 Android TextInput 仍有公开 issue/历史兼容性问题，所以 Android 文本编辑必须作为独立测试阶段，不能假设 Windows 文本编辑能力可以 100% 原样搬过去。对 Android 的 IME、光标、选区、删除、Enter、粘贴必须专门测试。

==================================================
二十九、MILESTONE
=============

M0 — Toolchain

完成：

* Rust
* Slint
* Windows build
* Release build
* GPU renderer
* Benchmark baseline

验收：

能启动。

GPU renderer 可确认。

性能数据可记录。

---

M1 — Design System

完成：

* Theme
* Typography
* Colors
* Icons
* Sidebar
* Toolbar
* Popup
* Button
* Context Menu
* Dark/Light

验收：

没有真实数据库也可以截图。

视觉必须达到产品原型级。

---

M2 — App Shell

完成：

* Sidebar
* Page tree
* Main editor area
* Search
* Settings
* page navigation

验收：

可以浏览 mock pages。

---

M3 — Local Document

完成：

* SQLite
* Workspace
* Page
* Block
* Auto save
* Load on restart

验收：

关闭应用再打开，数据完整恢复。

---

M4 — Block Editor MVP

完成：

* paragraph
* heading
* todo
* bullet
* numbered
* quote
* code
* divider
* Enter
* Backspace
* Delete
* Arrow
* Copy
* Paste
* Undo
* Redo

验收：

连续编辑 1000 blocks 不明显卡顿。

---

M5 — Notion Interaction

完成：

* Slash Command
* Command Palette
* Block menu
* drag/drop
* block reorder
* keyboard shortcuts
* selection/focus

验收：

核心交互连贯。

---

M6 — Rich Text

完成：

* bold
* italic
* inline code
* link
* strike
* basic marks

验收：

保存/加载不丢失格式。

---

M7 — Performance

完成：

* virtualization
* lazy rendering
* lazy loading
* debounce saving
* background search
* render optimization

测试：

1000

5000

10000 blocks

验收：

不存在明显的线性资源爆炸。

---

M8 — Windows RC

完成：

* crash recovery
* installer/package
* settings
* import/export
* performance regression
* keyboard shortcuts
* native Windows behavior

验收：

可以作为日常个人笔记软件长期使用。

---

M9 — Android

Windows 稳定以后才开始。

---

M10 — Block 扩充（§三十七 批次 A + B）

完成：

* image（粘贴 / 本地插入 / 附件落盘 / 宽度档）
* file + PDF 缩略图
* table（简单表格）
* toggle
* columns

验收：

图片页与表格页在 10 000 块场景下的 RAM 不劣于现有基线的 1.2 倍；每种新块六处接线齐（types / storage / md io / Turn into / slash / 截图场景）。

---

M11 — Block 扩充（§三十七 批次 C）

完成：

* code 高亮
* bookmark —— 2026-09-22 **撤回**，不做（ADR-0081）；卡片形状由 embed 承接（ADR-0040）
* embed 卡片
* math
* TOC

验收：

不新增 JS/WASM 运行时；高亮不得让长代码块的输入延迟可感知。

---

M12 — Page 外观与属性（§三十八）

完成：

* icon —— 2026-09-22 已交付，ADR-0045（`pages.icon`）；原文里的「+ 本地图片」由 ADR-0046 决定不做在 icon 上，图片归 cover
* cover —— 2026-09-22 已交付，ADR-0047（`pages.cover`，schema v12，可空，存 AttachmentId；固定遮罩把 §二十一 的可读性要求变成算术）
* font（default / serif / mono）/ full width / small text —— 2026-09-21 已交付，ADR-0044
* lock —— 2026-09-22 已交付，ADR-0048（`pages.locked`，schema v13；两层门：Rust 拒写 + .slint 拒光标）
* version history —— 2026-09-22 已交付，ADR-0091（**不加迁移**：一个版本就是 §二十五 那条 `VACUUM INTO` 收窄到一页后落在 `versions/` 里的一个文件，读回来用的还是 `Repository::load`，所以「不另造一套存储」是结构上的而不是纪律上的；索引是每版两行 metadata——名字与它指着哪几个附件，好让 §三十七 的回收看不见那条指针时不会把版本喂成一张丢图的页；`MAX_PER_PAGE = 20` 每页各算，磁盘与 RAM 数字在 docs/PERFORMANCE.md：60 行的页 122 880 字节 = 库的 1%，二十版 5 000 行的页 = 库的 159%，RAM 是「一次只有一版在内存里」。三个动作三个面：命名是输入框、对比是点一行、恢复是看过之后才出现的按钮，且它是**一次 Ctrl+Z**、不回滚页的名字与长相）
* 模板按钮 + 模板库 —— 2026-09-22 已交付，ADR-0049（`pages.template`，schema v14：模板就是一张页，看不见靠不挂树；`InsertForest` 让一次插入 = 一步撤销；五个预置经 §二十六 通道导入而不是写进 migration。顺带修掉 §二十六 一处真缺陷：空的列表项在导出时会整行消失）

验收：

schema migration 从旧库升上来不丢数据；锁定页任何输入都不被静默吞掉。

---

M13 — 引用层（§四十）

完成：

* @page mention、@date
* 反向链接区
* synced block

验收：

索引增量维护，打开页面的反向链接计算不随全库规模线性变慢。

---

M14 — Database（§三十九）

按 §三十九 的视图顺序内部再切里程碑：先 table 视图 + 属性模型，再 board / list / calendar，再 gallery / timeline / form / chart，最后 relation / rollup / formula 与 linked database。

验收：

10 000 行库的 RAM、视图切换耗时、公式编辑面板耗时全部进 docs/PERFORMANCE.md；filter/sort 在 SQL 侧。

---

M10 与 M9 的先后：M9 已按 2026-09-20 的决定暂停，所以 M10 起可与 M8 尾巴交错排入。每个里程碑仍须走 §三十 的同一套流程：读架构 → 最小改动 → 编译 → 测试 → 检查 UI → 记录性能影响。

==================================================
三十、Agent 工作规则
=============

你不是一次性写完所有代码。

严格按照 milestone 推进。

每一次修改都应该：

1. 阅读现有 architecture
2. 找到相关模块
3. 修改最小范围
4. 编译
5. 测试
6. 检查 UI
7. 记录性能影响

不要：

一次生成 5000 行代码。

如果某个阶段变得复杂，先拆成子任务。

==================================================
三十一、Agent 的 UI 开发规则
===================

UI 是这个项目的重要竞争力。

你必须愿意多次迭代 UI。

不要因为：

“功能已经能用”

就停止。

要不断检查：

视觉层级

间距

字体

颜色

hover

focus

selected

动画

响应速度

窗口 resize

长文字

中文

英文

高 DPI

125%

150%

200%

---

## 视觉风格

整体目标：

Modern minimal productivity app

而不是：

传统企业软件

也不是：

Material Demo

也不是：

Fluent UI Demo

也不是：

“把 Notion 原样复制”。

---

## 三十二、Agent 的性能规则

任何新增功能都要问：

“它是否会长期占用 CPU？”

“它是否会增加常驻 memory？”

“它是否造成额外 render pass？”

“它是否导致整个 document rebuild？”

“它是否创建大量 UI Item？”

“它是否增加 timer？”

如果答案是“会”，必须说明原因。

---

## 三十三、特别禁止

禁止：

Electron

Tauri

WebView

Chromium

React

Vue

HTML UI

CSS UI

Node runtime

大规模 JS

Qt WebEngine

任何不必要的后台轮询

无限动画

每帧业务逻辑

10000 个永久 TextEdit

每字符 SQLite transaction

整个 Document rebuild

UI 直接 SQL

UI 直接文件 IO

自己实现 Windows IME / TSF

第一阶段实现云同步

第一阶段实现实时协作

第一阶段实现 AI

第一阶段实现插件系统

第一阶段实现发布为公开站点

第一阶段实现评论、讨论与表情回应

---

2026-09-20 澄清：以上六项（云同步、实时协作、AI、插件、发布站点、评论）是唯一被明确排除的能力。§三十七 至 §四十 的块扩充、页面属性、Database、引用层都是要做的，不因 §三十五「功能数量排最后」而被无限推迟——该条只约束「不得为堆功能牺牲 UI 质量与 RAM/CPU」，不构成砍范围的许可。

==================================================
三十四、文档维护
========

必须维护：

PLAN.md

ARCHITECTURE.md

UI_ARCHITECTURE.md

EDITOR_ARCHITECTURE.md

PERFORMANCE.md

DECISIONS.md

每个 Milestone 完成后更新。

DECISIONS.md 记录：

为什么选 Slint

为什么选某个 renderer

为什么不用 WebView

为什么不用 Flutter

为什么 Block Editor 采用当前模型

为什么采用 virtualization

为什么某个依赖被加入

为什么某个依赖被移除

这样以后 Agent 接手项目时，不需要重新推断整个架构。

==================================================
三十五、最终产品目标
==========

最终产品应该具备以下感觉：

打开软件：

启动快。

窗口响应快。

静止状态：

CPU 很低。

页面滚动：

流畅。

长页面：

不会因为 Block 数量增加而发生明显资源爆炸。

输入：

即时响应。

中文输入：

正常。

UI：

现代、克制、漂亮。

Sidebar：

精致。

Command Palette：

快速。

Block：

有良好 hover / selected / focus feedback。

页面：

像成熟知识管理产品，而不是工程 Demo。

最终不要追求：

“功能最多”。

而要追求：

“核心体验非常完整”。

整个项目最重要的优先级是：

1. UI 视觉质量
2. 编辑体验
3. Windows 低 RAM
4. Windows 低 CPU
5. GPU 渲染
6. 架构可维护性
7. 功能数量

不要为了增加功能数量破坏前五项。

==================================================
三十六、第一轮执行要求
===========

现在不要直接开始实现完整产品。

先只完成：

M0 + M1。

也就是：

1. 从 slint-rust-template 建立工程
2. 确认 Slint 当前稳定版本
3. 配好 Windows Release
4. 验证至少两个 GPU renderer
5. 建立 benchmark baseline
6. 建立目录结构
7. 建立 ARCHITECTURE.md
8. 建立 PERFORMANCE.md
9. 建立 PLAN.md
10. 建立 Theme / Colors / Typography / Icons
11. 做出完整 App Shell
12. 做出 Sidebar
13. 做出 Page Tree
14. 做出 Main Editor placeholder
15. 做出 Command Palette mock
16. 做出 Light/Dark theme
17. 完成一次视觉打磨
18. 编译 Release
19. 测试空闲 CPU/RAM
20. 输出 M0/M1 完成报告

完成这些以后再进入 Document Model 和 Block Editor。

不要跳过 M0/M1 直接写数据库和编辑器。

整个开发过程以“可运行、可测量、可回退、可持续扩展”为原则。

==================================================
三十七、第二十三阶段：Block 类型扩充
==============

§九「后续再添加」的那批块从本阶段起进入排期，不再是开放问题。

已交付，不需要再做：

callout（表情 + 底色）

page / link-to-page，共享 blocks.page_ref（ADR-0026）

toggle（批次 B 第一项，ADR-0028：折叠子树零 realized row，row→model 换算缝
`visible_block_indices`）

image（批次 A 第一项，ADR-0029：附件目录 + `attachments` 表、`MAX_EDGE` 降采样
缓存、25/50/100 宽度档位、点击预览；ADR-0035：从剪贴板粘贴已接——Ctrl+V 读
`CF_DIBV5`/`CF_DIB` 并在 `platform/dib.rs` 里解成 PNG，空块直接变成图片、有字的块
在下方得到图片，剪贴板同时有文字与位图时文字优先。「一页图片被滚动时的实测」
已交付（ADR-0036，`--pictures N` 基准场景 + 应用自报解码缓存）：32 MiB 预算按
字节权重停在 9 张 1280×720 栅格（98.9 %），滚到它之前一页图片的代价与无图页
一致，而屏幕上一张图片约占 9 MB 进程内存——是栅格的三倍，所以约束是视口不是缓存。
本 kind 剩下的未做项只有真实照片熵与 HiDPI 下的同一批数字）

file（批次 A 第二项，ADR-0030：不加 schema，复用 v7 的 `attachments` 行与
`blocks.attachment` 列；`fs::copy` 流式落盘、全程不解码不设上限，所以 2 GB 附件
与 2 KB 附件占同样的工作集；名字保留扩展名、体积走 `attachment-size` 回调、
Open / Save-as 两个显式按钮（`ShellExecuteW`，不 spawn explorer）；Markdown
导出写成链接而不是图片形状，因此能过导入器往返。PDF 首页缩略图按 2026-09-20
用户指示推迟，未做）

table（批次 B 第二项，ADR-0031：`blocks.columns` + v8 迁移，单元格是 Table 的
子块（`table_cell` kind）、行主序平铺，所以 rows = cells / columns 是派生值不是
存储值；Tab / Shift-Tab 跨格、越出末格增行，hover 边缘条增删行列；⋮⋮ Turn into
双向（转成表格时整行文字进左上格，展平时格子变回段落）；Markdown 导出 GFM 表格，
导入侧 `|a|b|` 仍按段落处理并有测试钉住。明确不是数据库视图）

columns（批次 B 第三项，ADR-0032：不新增迁移，`columns` 块的 `blocks.columns` 存栏数，
栏是它的 `column` 子块、栏内的行是栏的子块；投影把 layout 的内容摊成一个 `column-items`
模型加一个 `column-boxes` 形状表，因为 Slint 没有递归组件，layout 的 row delegate 就是
它自己那一条，所以分栏只在可见窗口内展开；平铺交给 Slint 1.18 的 FlexboxLayout，
`alignment: stretch` + 每格 `horizontal-stretch: 1`；hover 边缘条增删栏（2 栏以下拒绝、
3 栏以上拒绝），删栏时栏里的字回流到前一栏；Markdown 导出摊平成页面级段落）

顺序按「日常笔记撞墙的速度」排，不按 Notion 的字母表排。

---

## 批次 A：媒体

image：

* 从剪贴板粘贴、从本地文件插入（rfd 已在依赖里）——2026-09-21 两条都做了：剪贴板
  走 `platform::read_clipboard_image` + `platform/dib.rs`（ADR-0035），文件走 `rfd`
  选择器；只认位图格式 `CF_DIBV5`/`CF_DIB`，不读 `CF_HTML`/`CF_RTF`，也不读应用
  自注册的私有图片格式或资源管理器复制文件时的 `FileGroupDescriptorW`
* 可选格式就是 `image` crate 特性里显式声明的那四种：png / jpeg / bmp / gif。
  gif 是静图——解码器交回首帧，编辑器里没有动画时钟，也不打算有；其余解码器
  能读的格式（webp / tiff 等）不进选择器，因为它们的行扩展名只能落到 `.img`
  兜底，那属于「能存但说不清」，不做
* 落盘到附件目录，数据库只存引用（attachments 表，§十八已预留）
* 显示宽度可调（25 % / 50 % / 100 %），点击进入预览
* 缓存与降采样计入 §二十二 的性能预算：一张 4000×3000 的原图不得以原始尺寸常驻内存

file：

* 任意文件附件，显示文件名 + 体积 + 打开 / 另存为
* 音视频交给系统默认播放器，Quire 不做播放内核
* 附件字节不得进入进程：流式复制落盘、只记长度，不设体积上限也不解码
  （§二十二 优先于功能数量）

PDF：

* 先按 file 处理 + 首页缩略图；内嵌翻页阅读器不在本阶段
* 2026-09-20 用户指示：首页缩略图这一半先跳过，PDF 目前只走上面的 file 路径，
  所以 `.pdf` 与 `.zip` 除文件名外长得一样；缩略图的渲染路线待定

附件回收（2026-09-21 交付，ADR-0037）：

* Settings → STORAGE 一行的 Reclaim 按钮：删掉没有任何引用的 `attachments` 行，
  行落库之后才删文件，结果用 `db-notice` 一行报出来（删了几个、释放多少字节）
* 「有引用」算得比屏幕上宽：全部页面的块（不只是打开那页）+ 任何页面 undo **或**
  redo 栈里的步骤 + 内部剪贴板复制的那一块。所以撤销契约一字不改——栈上还留得下
  100 步，就还保护这 100 步；超出上限的老步骤不再受保护，这是承诺的边界
* 只认数据库里读到的行，绝不列附件目录：`load_attachments` 失败时内存账本是空的，
  列目录就等于把整个图库看成孤儿删掉
* 在 UI 线程上同步跑：1 000 个孤儿约 0.5 s，可达性扫描不在时钟上（`docs/PERFORMANCE.md`）。
  这个量级不做进度条——提示条就是反馈，而且再点一次不会多删任何东西
* 仍然没做：没有行的裸文件（导入写成功、行没落库那一类）回收不了，属已知限制

---

## 批次 B：结构

table（简单表格，不是 Database）—— 2026-09-20 已交付，ADR-0031：

* N×M 单元格，Tab 跨格，最后一格 Tab 增行，增删行列
* 单元格内是纯文本 + §十 的 inline marks
* 明确不是数据库视图；schema、过滤、排序属于 §三十九
* 交付时确定的边界（都不算缺陷，见 ADR-0031 的 Consequences）：单元格内没有
  Enter 拆行、没有退格合并、没有上下键跨行，只有 Tab / Shift-Tab 走格；格子里做不了
  链接（Ctrl+L 未接），加粗/斜体/行内码/删除线可用；把一行带 marks 的文字转成表格时
  字留下、marks 丢掉；删除最后一行 / 最后一列被 plan 拒绝（表格至少 1×1）；
  残缺网格（cell 数不是 columns 的整数倍）只读不可编辑

toggle（折叠块）：

* 任何块都能成为 toggle 父级，折叠状态入库
* 折叠掉的子树不得保留 realized row（对齐 §十二 的 virtualization 前提）
* 折叠是视图状态：只入库一个 folded 标志，Markdown 导出照常带上子树（§二十六），导入侧不还原
* 只有 Toggle 画三角：把一个折叠着的块 Turn into 成别的种类，必须顺手展开它，否则子树回不来

columns（分栏）—— 2026-09-20 已交付，ADR-0032：

* 2 / 3 栏，栏内为块序列
* 用 Slint 1.18 的 FlexboxLayout，不自己写排版
* 与 §十二 冲突时优先保虚拟化：分栏只在可见窗口内展开
* 交付形态：layout 是 `columns` 块，栏是它的 `column` 子块，栏里的行是栏的子块，
  所以整套结构不需要新迁移（复用 v8 的 `blocks.columns` 存栏数）
* 交付时确定的边界（都不算缺陷，见 ADR-0032 的 Consequences）：栏内的 ↑/↓ 只动光标、
  不会离开 layout，Tab / Shift-Tab 在栏与栏之间走字但到两端就停住（出栏靠点击）；
  空栏只写「Empty column」，点它才生成第一行；Markdown 导出把分栏摊平成页面级段落
  （形状丢失、文字全留），导入侧本来就没有分栏语法

---

## 批次 C：引用与嵌入

bookmark：~~链接卡片，抓标题与 favicon；离线或抓取失败退化为纯链接，且不得阻塞输入~~
—— 2026-09-22 **撤回，不是推迟**（ADR-0081）

* 更正一条依据：Quire **有** HTTP 客户端（`src/services/lan_client.rs`，无依赖的
  HTTP/1.0 `GET`，给 `main.rs` 的 `--pull <url>` 用），所以准确的说法是
  **没有 TLS、也没有一次不是用户刚刚要求的请求**——缺的是传输安全与同意，不是
  「这个应用不联网」
* 要走这条路就要在同一个里程碑里再进一棵 `rustls`（或 `native-tls`）依赖树，先回答
  「Quire 会不会自己发请求」（同意），再定标题与 favicon 的落盘、容量与淘汰规则
* 而 SPEC 真正要的卡片形状——表明链接属于谁、显示将要交出去的地址、离线降级——已由
  **不抓取的** embed 卡片交付（ADR-0040）。差的那一点正好是需要网络的那一点

embed：YouTube / Figma / Google Maps 一类链接转卡片。本阶段只做占位卡片 + 外部打开，不做内嵌浏览器（§二 与 §三十三 禁 WebView）—— 2026-09-21 已交付，ADR-0040

* 交付形态：`BlockKind::Embed`（int 22，库里字符串 `"embed"`）的 `text` 存的就是那个地址本身；
  卡片上的两行（属于谁 / 打开什么）是绘制时由 `core::embed` 两个纯回调现算的，模型里没有副本
* 没有内嵌浏览器，所以「卡片 + 交给系统打开」就是功能的全部，不是它的降级形态
* Markdown 里是一行裸地址（GFM 自动链接）：任何渲染器都把它显示成链接，包尖括号或写注释
  标记只是多一个会被写坏的东西；导入侧只认「整行一个 token 且带 `http(s)://`」，
  句子里的地址仍然是句子
* 边界（不算缺陷）：不抓标题、不抓 favicon、不预览——`bookmark` 已于 2026-09-22
  撤回（ADR-0081），所以这不是「还没做」，卡片显示的是域名而不是网页标题；识别不了的
  域名就以域名本身为标签；未填地址时卡片说「No address yet」

code 高亮：—— 2026-09-21 已交付，ADR-0042

* 纯词法着色，先覆盖 rust / python / js / ts / md / json / bash
* 不得为高亮引入 JS 运行时
* 交付形态：块存的是「用哪门语言着色」（`blocks.lang` 一列，v9 迁移，`Change::BlockLangSet` 可撤销，
  ⋮ 的 Language 子菜单只在 code 块上出现），字是绘制时派生的：`core::highlight::layer` 把整块文本
  连同硬换行算成六份等长字符串，每份只留一种颜色的字符、其余换成不换行空格，Slint 侧六层 `Text`
  逐字叠合，所以没有「两个排版器要对齐」这回事——runs 通道（ADR-0041）一个 run 是一格、不能断行，
  当初正是这面墙把高亮挡在外面
* 行高仍由 kind 0 那层量：量行的字符串与上色的字符串是同一份，多出来的最坏情况是一行空白，不是裁掉的页
* 一个字符多宽只问字体一次（`Editor.slint` 里一个不可见的探针 `Text` 写 `UIState.code-advance`），
  不在行内问：ListView 显示哪些行取决于滚动位置，而一块代码在哪断行不该取决于它
* 着色只在块没被编辑时发生（`is-highlighted && !editing`），所以每敲一个键不做一次词法分析；
  那五层是条件元素而不是 `visible: false`，因为后者拦不住 binding，等于每一行每次重绘都跑五遍词法
* Markdown 是围栏上的 info string：```` ```rs ```` 读进来是 Rust，导出写 ```` ```rust ````；认不出的语言
  折成 `Plain`，也就是不着色，而不是一个坏块
* 边界（不算缺陷）：非 ASCII 字符宽度未知，所以六层都原样带上它、注释层最后画 —— 一行里既有注释又有
  中文散文时，那串中文是灰色；列数按等宽字体算，`clip: true` 是这条假设的地板；没有语义分析，
  `Vec` 与 `println!` 是名字不是类型检查

math：—— 2026-09-21 已交付，ADR-0038

* inline math 与 math block，LaTeX 子集
* 渲染优先 Unicode 近似排版；要引入排版引擎必须先出 ADR 并附内存数字
* 交付形态：库里存的是**源码**（`math` 块的 `text`、`math` mark 覆盖的字节区间），
  渲染串是每次投影/求值时由 `core::math::to_unicode` 派生的，所以 TextEdit 与
  两向绑定照旧说真话，也不加迁移
* 交付时确定的边界（都不算缺陷）：没有排版引擎，所以 `\frac` 是一行的 `a/b`、
  积分和求和只是符号加上下标；`\begin{pmatrix}` 一类环境整段原样回显（看不见
  的东西才算丢）；不认识的命令原样回显；源码里的空格是内容不是语法，`\alpha \, \beta`
  的空格会留在渲染串里；行内公式与粗体/斜体重叠时导出以 `$…$` 为最外层，被它包住的
  其它 mark 在导出时丢掉（公式内部不叠加样式，字一个不少）

TOC（目录块）：由当前页 heading 实时生成，点击跳转，派生数据不入库 —— 2026-09-21 已交付，ADR-0039

* 交付形态：`BlockKind::Toc`（int 21，库里字符串 `"toc"`）只存「这里有一个目录块」这件事；
  列表是每次投影时从本页标题现读的（`toc_entries`），所以改标题就等于改目录，不存在过期副本，
  也不需要迁移
* 被折叠或被容器藏起来的标题不进目录：目录复用的就是投影自己算出来的那份可见行清单，
  点不到的链接比没有链接更糟
* Markdown 里是一行标记 `<!-- quire:toc -->`：写进派生数据等于把本库的 block id 塞进别人的文档，
  而这一行在任何渲染器里都不显形，读回来还是它自己
* 边界（不算缺陷）：只列本页标题，没有「显示三级以下 / 紧凑模式」开关；一行一条，过长截断；
  点击走 M8 就有的 `quire://block` 那条应用内跳转，所以它**移动光标但不滚动视口**——
  Slint 1.18 的普通 `ListView` 没有 `bring-into-view`，锚点跳转本来就有同一条限制

synced block：依赖 §四十 的引用基础设施，排在它之后

---

## 硬性约束

每加一种块必须同时改到：core/types.rs 的 BlockKind、storage 的 kind 字符串、Markdown 导入导出（§二十六）、⋮⋮ 的 Turn into、slash 菜单、截图场景。

少一处即视为未完成。

凡是「行是动态的」块都要多改两处，因为（2026-09-20 实现 toggle 时确认，table
于 ADR-0031 命中同一条）：

* projection（`project_blocks`）必须真的把隐藏的子树从 rows 里删掉，不是留一个 `visible: false` 的 delegate
* 凡是拿 row index 当 model index 用的地方（现在是 §八 的拖拽落点）都要做一次 row→model 换算；折叠一发生，这两个编号就不再看同一个位置

==================================================
三十八、第二十四阶段：Page 外观与属性
============

§十七 的 Page Tree 只管结构，不管页面本身长什么样。本阶段补上。

数据前提：pages 表加列（icon / cover / font / layout / locked / template），走 §十八 的 migration，schema 版本 +1，旧库必须能无损升上来。—— 2026-09-21 落了 font + layout 两列（schema v10，ADR-0044），2026-09-22 落了 icon 一列（schema v11，ADR-0045），同日落了 cover 一列（schema v12，ADR-0047，可空而不是 `DEFAULT 0`：`0` 是一个合法的 AttachmentId，「没有封面」必须是第三种值），同日落了 locked 一列（schema v13，ADR-0048，`NOT NULL DEFAULT 0`），同日落了 template 一列（schema v14，ADR-0049，同样 `NOT NULL DEFAULT 0`：「不是模板」也是一个值）；这些步骤共用一个 `add_page_columns`，所以一个半途的库（有人手工加过列、或从备份恢复到步骤中间）是收敛而不是报错。

## 图标与封面

icon：emoji 选择器 + 本地图片；未设置时用标题首字符占位，侧边栏与页面标题同步显示 —— 2026-09-22 交付 emoji 选择器那一半，ADR-0045（`pages.icon`，存 emoji 本身而不是选择器的下标）。本地图片那一半见下面第二段的决定。

「占位」这句按位置读成三条，因为三处的空槽含义不同（ADR-0045）：树里的行 = 标题首字符（那里的槽原本是一个跟这页无关的通用页图标，首字符才「说了点这页的事」）；Favorites / Recent 的行 = 只显示存下来的 emoji，没有就保留星形与时钟（那两处的图标是「这一节是什么」的记号，被首字符吃掉是丢信息不是占位）；编辑器大标题上方 = 什么都不画（把标题首字符再放大 60px 画一遍是回声，不是占位）。「同步显示」落在写入路径上：一次 `set_page_icon` 同时刷新 hero 与整棵侧边栏，两处读的是同一个存储值。

「+ 本地图片」这一半 2026-09-22 决定**不做在 icon 上**，ADR-0046：附件只有一档 `MAX_EDGE = 1280` 的降采样（为 ~780px 的正文列准备的），塞进 16px 的侧栏槽位要么让每次侧栏重建去解一张 6.5 MB 工作集的图，要么加第二档缓存——而第二档意味着 M10 的回收扫描要多认识一个引用者（icon 是**页**指向附件，不是块），否则用户的图标会被当成没人指的文件回收掉。图片的去处是下面的 cover，那里本来就要求换图 / 移除与标题对比度。

cover：本地图片，可换图 / 移除；封面之上的标题对比度必须过 §二十一 的可读性要求，不得用最弱配色 —— 2026-09-22 交付，ADR-0047（`pages.cover`，schema v12，存 **AttachmentId** 而不是路径：§三十七 的回收扫描只能从数据库里回答「还有谁指着这个文件」，一列路径是它解不回来的字符串，于是用户自己的封面会变成下一次回收的牺牲品）。换图 / 移除都在页面 ⋯ 里，移除只在有封面的时候存在。

「不得用最弱配色」读成一条可以证伪的话，而不是一个态度（ADR-0047）：应用不可能在每次开页时按 hero 尺寸解一张任意 JPEG 去问它最亮的像素是多少，而逐图自适应的色又是一个 ADR-0039 拒绝给它存储的派生值。所以用**一层固定遮罩**把要求变成算术——`#0000009e`（黑，α 是一个字节：`0x9e` = 158，α = 0.6196），任何照片像素合成后 ≤ 255 − 158 = **97** sRGB，相对亮度 ≤ 0.1195，白字在上是 **6.19:1**，而 4.5:1 这条线本身要求 α ≥ 0.535。最坏情况是一张纯白的图，所以 `page-cover-white` 就是照着它拍的，`benchmarks/scripts/contrast_probe.ps1` 从渲染出的 PNG 上量比值（实测最坏底色 `#616161`，152 002 px 采样），并且脚本自带 known-answer 臂（21:1 / 1:1 / 6.19:1）与一个 1.61:1 的必然失败臂——门禁要能说不，才算说过。封面之上的字**与**页面自己的 emoji 都走 `text-on-accent`：第一次量图就抓到漏掉的那个，一个 `#1f2328` 的火箭坐在 `#232439` 的照片上，1.04:1。

## 页面版式

font：default / serif / mono 三档，按页生效 —— 2026-09-21 已交付，ADR-0044（`pages.font`）

full width：页级开关 —— 2026-09-21 已交付，ADR-0044（`pages.layout` bit 1）

small text：页级开关 —— 2026-09-21 已交付，ADR-0044（`pages.layout` bit 2）

三者只作用于当前页的排版 token，不得下沉成 per-block 字号。—— 交付方式：三档开关存在页上，`PageType` 全局从 `UIState` 派生出文档层的 token，119 处调用点从 `Typography.*` 改读 `PageType.*`，chrome 仍读 `Typography`；block 表与 `Block` 结构一个字段都没加。入口是顶栏 ⋯ 菜单的 Style 子菜单。

## 锁定与版本历史

lock：只读开关。TextInput、slash 菜单、拖拽、⋮⋮ 的编辑项全部关闭，并且给出可见的锁定状态，不能静默吞输入。—— 2026-09-22 交付，ADR-0048（`pages.locked`，schema v13，`INTEGER NOT NULL DEFAULT 0`：「没锁」是一个值而不是一种缺席，所以这列不像 cover 那样可空，迁移也就是一条不用回填的 `ADD COLUMN`）。

句子里的四个关闭点读成**两层门**（ADR-0048）：Rust 那一层拒的是写——`exec_editor` 那个所有块命令共用的漏斗，加上漏斗旁边的三个入口（checkbox 的回调直接进命令层；`clear_block_ref`；`duplicate_page_block` 在命令层看到块之前先经树造出一个子页）。.slint 那一层拒的是光标——`EditorBlock.editing` 多出一个 `!UIState.page-locked` 项，因为一个陈旧的 `editing-id` 会在重新加锁之后仍然留着一个活输入框；只在命令层设门，等于允许一个「按 Enter 才说不」的文本框存在。撤销也在锁后面，但**拒的不是丢**：锁之前建起来的 undo 栈原样留着，解锁之后接着用。

「⋮⋮ 的编辑项全部关闭」读成**删掉**而不是置灰，因为那个菜单没有 disabled 状态可以说「不行」；留下的是两行只取的：Copy link to block 与 Copy block。「给出可见的锁定状态」是三处像素：标题上方一颗 pill（"Page locked · ⋯ to unlock"）、⋯ 里那一行自己变成 Unlock page、以及变短的 ⋮⋮。「不能静默吞输入」由通知条承担，而去重的依据是**屏幕上那一行字**（bar 是粘的， dismiss 才走），因为拖拽的 hover 每个重绘帧都要问一次 `can_move_block_to`；hover 自己不说话——落点线消失就是那个手势的反馈，紧随其后的那一次 drop 才值得一句。

不在锁里的：页面自己的外观（icon / cover / Style / favorite 照写，那是 §三十八 前面几段的特性）、树操作（移动 / 删除 / 复制页面）、以及一切**读**（导航、搜索、展开折叠）。`ToggleFold` 因此是锁内唯一仍然执行的命令——§三十七 把它登记成持久化的*视图*状态，加锁不该让用户失去这页的大纲。门里写着一个具名例外，所以它由一条断言钉住：其余十条命令返回 `None` 的同时 `ToggleFold` 必须还是 `Some`。复制页面**不**带上这把锁，与带上字体 / icon / 封面相反——外观跟着走是因为副本该长成源的样子，锁不跟着走是因为「复制一个locked的页」正是用户想改它又不动原版的动作。

version history —— 2026-09-22 交付，ADR-0091。下面三条就是本节对它的原文要求，每条后面接它落成了什么：

* 复用 §二十五 的 snapshot 机制，不另造一套存储 —— 读成**照抄那一条语句**：一个版本就是 `backup::snapshot` 的 `VACUUM INTO` 副本（同一个 `synchronous=OFF` 的交易，同样靠 `Database::open` 回来验一遍），然后**收窄到一页**——删掉别的页（`blocks.page … ON DELETE CASCADE` 连带块行、`block_children`、`marks` 一起走）、清空两张 FTS 表、再 `VACUUM` 把省下的页交还给文件系统。于是「不另造一套存储」不是一句态度：`versions::read` 用的就是应用开库时那个 `Repository::load`，一个版本**就是**那个格式，所以它无从与格式走散。代价写在这里：一个版本是一个文件，`user_version` 停在 14，这一片一条迁移都不加——M12 前五刀每刀一列（v10…v14），这一刀什么列都没有。
* 用户可见：命名版本、与当前版本对比、恢复 —— 三个动作落在**三个面**上（一个 popup 的两个视图）：命名是底部那行输入框（永远在，因为「现在」随时值得一版），对比是点一行，恢复是**只在对比之后才出现**的那颗按钮——把一页换掉之前，用户得先看过它变成什么。行带的是**行号**不是时间戳，因为 Slint 的 `int` 是 32 位而 unix 秒不是。对比是**行级**的、以 block id 认身份（快照带着这页自己的行，所以没动的行两边同 id），编辑一行只付一次比较而非每行一次（先剪公共头尾）；中间段超过 `ALIGN_CELLS = 1 << 18` 个配对就**报成整片重写**而不是算完——多出来的行仍然句句为真，而一个要等一秒才开的面板比一个多显示几行的面板更坏。恢复走命令系统而不是换文件：一批 `exec_all`（先 `InsertForest`，再逐个删当前的**根**，子树跟着走）= **一次 Ctrl+Z**，新行另发 id、重排 order key，flush / FTS / 重启看到的只是一页寻常地变了。**页的名字与长相故意不回滚**——版本是页的*内容*的版本。两道门都是继承来的：`locked_refusal()`（ADR-0048）与「只有屏幕上这页能恢复」。
* 保留策略必须给出磁盘与 RAM 数字，不接受无限增长 —— `versions::MAX_PER_PAGE = 20`，每页各算，从**最旧**那头砍，而且面板自己把数字念出来。数字与测量命令在 docs/PERFORMANCE.md：9.9 MB 的库里，一页 60 行的版本是 122 880 字节（1%），5 000 行的是 790 528（7%），二十版长页是 15 810 560（**159%**）；RAM 侧是「一次只有一版在内存里」——一条短暂连接（SQLite 默认页缓存 ≈2 MB，Quire 不改 `cache_size`）加上两侧各一份 `Block`（实测 136 字节/块，5 000 行 = 680 000 字节）与那张 ≤1 MiB 的对齐表。顺带一条本可以溜过去的：**空 FTS 表不等于空索引**——`DELETE FROM search_blocks` 之后行数报 0，`search_blocks_data` 仍留着 368 行 / 1.4 MB 的全库用词，于是「一页的快照」在字节上还是「整个库的副本」；现在跑完 `DELETE` 再发 FTS5 的 `rebuild` 命令（`delete-all` 更短而这里被 SQLite 拒绝：它只给 contentless / external-content 表用），断言也从「你会去查的那两张表的行数」搬到了 `_data` 上——因为骗人的正是那个行数。

## 模板

页面内模板按钮 + 新建页面时选模板 —— 2026-09-22 交付，ADR-0049。页面内那一半开在**两个**已经存在的入口上：slash 菜单与「+」把手的插入菜单（`open_slash_insert`），两者的候选表尾都接上模板库（`template_slash_rows`），hint 列写死一个 `Template` 词，因为模板不在树里、没有面包屑可给；`TEMPLATE_SLASH_BASE` 远在任何块类型整数之上，所以 `slash_selected_kind` 能分清「这行是模板」与「这行是它不认识的类型」——后者会被 `kind_from_int` 折成 Paragraph，那是唯一一种**静默**错法的结局。新建页面时选模板那一半是 ⋯ → Templates → Use as new page：`create_page` 然后一次插入，于是新页从它第一个变化起就是一张普通页（在树里、在搜索里、在自己的 undo 栈上），模板继续藏在它后面，标题取模板的名字（一张用户刚挑过形状的用「Untitled」什么也不说）。

workspace 模板库：预置若干本地模板，导入导出走 §二十六 的 Markdown 通道 —— 五个预置（`core::template::PRESETS`：Meeting notes / Weekly review / Project brief / Bug report / Long-form draft），按「多久会有人需要」排序而不是字母序。它们是**内容而不是资源**：不读文件、不下载，任何库都带着这五个。落库的方式是 `seed_builtin_templates` 而不是 migration（ADR-0049）：v14 只加列就停，因为自己写块行的迁移得手工对齐 order key、`block_children` 与两张 FTS 表，而菜单里 Import 那一行调的 `import_template` 已经把三件事都做对了 —— 于是内置库是被**导入**的，不是被发明的，可能出错的代码路径只有一条。两道门各管一个方向：settings 旗标 `builtin-templates-seeded` 让「删掉五个」这件事活得过重启（否则菜单的 Delete 行是一句谎），名字检查让一次半途而废的 seed 可以重试（旗标在正文**之后**才记，否则什么都没写进去的会话会旗标为已种而永远缺图）。没有库可种的会话（`persistence.is_none()`）直接拒绝——headless 的 bench 场景与视觉捕获因此自己画一份库，而不是看见这五个。

模板的表示必须是「块序列的副本」，不得引入第二套内容格式。—— 交付方式是遵从而不是对抗：模板**就是**一张页，`pages.template` 一列（schema v14，`NOT NULL DEFAULT 0`，形状与 v13 的 locked 一模一样）是它身上唯一一条普通页没有的事实，正文与所有页面共用 `blocks` 表。于是 marks / 颜色 / `lang` / `columns` / `img_percent` / 一个 `Page` 引用在复制时一律随行，一行新的映射代码都不需要；`fill_template` 二十行就够了，因为它 clone 的是行：id 由文档自己的分配器另发（副本永不会与源相撞），order key 原样留着（一个 key 只在一页之内有意义，而新模板没有东西可与它撞），parent 指针重映射到副本上——把一个表的格子、一个 toggle 的孩子绑在一起的正是这一步。代价也写在这里：预置模板只能装 Markdown 装得下的东西（§二十六 的解析器没有颜色 / callout / table / 分栏的语法，要了就会悄悄变成一个段落），而用户「Save as template」存下来的那份不同，它是真实行的副本，源页有什么就留什么。

「看不见」读成**不挂树**，而不是一长串过滤器（ADR-0049）：`create_template` 造一页、翻一个旗标，既不进 `roots` 也不进任何父亲的 `children`，所以所有走树的枚举器（侧栏、页面树、命令面板、Move-to、Recents）免费跳过它——没有一份「记得要过滤」的清单需要后来人维护。四把不走树的门各自补了一项：`open_page` 在 `mark_opened` 之前返回（打开会写 `recents` 与 `current-page` meta，那是模板再多不能出现的两个地方）；`search_index::matches` 的 join 上多一个 `AND p.template = 0`；LAN 分享过滤页列表与子页遍历，而 `/api/page/<模板 id>.md` 回答 **404** 而不是它自己的 Markdown，因为对岸没有办法表达「模板」，那会把它落成一张普通页。第五把门考虑过并且**拒绝**：不给模板的行建索引——那会让 `insert_block` 去问自己脚下这页是不是模板（一份 join 已经知道的事实出现第二个真相源），而 `rebuild` 必须与 `insert` 对「哪些行属于索引」持不同意见，于是模板会在一次重建之后开始出现在搜索结果里。钉住这条的测试除了断言命中为空，还断言 `search_blocks` 里的原始行数，好让「看不见」不能被解释成「没数据」。

`InsertForest` 是「一次插入 = 一步撤销」的那条命令（ADR-0049）：十一块的模板按一次 Ctrl+Z 就该全走，而不是留下九块；它产出的变化清单就是普通 `BlockInserted`，所以 flush、FTS 与重启看到的都只是「这页长了几行」。锚点上那行**空的** paragraph 是同批被替换掉的（「+」那一行与新建页的第一行都是空的，模板落在光标下面一行看起来像没生效），这只有 `exec_all` 拿前态给每条命令做计划才成立；它返回第一个插入块的 id，因为这条命令可能**删掉**用户点的那一行，光标留在一个已不存在的块上是一次点击之外的编辑失败。`fill_template` 不产生 undo 步骤，这是设计而非缺口：历史栈属于用户正在打字的那一页，而模板页从来不是开着的——存错的回头路是 Templates > Delete。

入口是页面 ⋯ → **Templates** 一行（菜单因此十三行；一行而不是六行，因为那一行的标签就是特性的名字），子菜单六行：Insert template / Use as new page / Save as template / Export Markdown / Import Markdown / Delete template。标签短是被迫的而且是可查的：全应用共用一个 184px 的 `ContextMenu`，它的行是 elide 而不是换行，第一版渲染里六行有四行结尾是省略号——对象已经被用户站着的那个子菜单和紧随其后的库选择器各说了一遍，所以留下的是能一眼读完的那一行。没有「edit template」那一行，因为改一个模板的方式就是拿它开一页、编辑、再存回去；保存**从不**覆盖，所以旧的那份会留在库里直到用户删掉它，而库里可以有两个同名模板（按年龄排而不是按标题排）。Delete 不再弹确认框（那是用户从自己起的名字里挑的第二下点击，行本身已经画成危险色），但它必须说话：通知条点名被删掉的是哪一个。`delete_page` 回答的是「删掉的是不是屏幕上那页」而不是成没成功，对模板恒为 `false`，所以测试断言的是前后两次 `workspace.contains()`。锁页拒绝插入（门在 `exec_all_on_open_page` 上，不需要模板专属的一项）但照样提供「Save as template」——把正文抄出去不是往这页里写。回收扫描（§三十七）一项都没加：模板的行就是 book 里的块行，`doc.all_blocks()` 已经把它们指着的图算作引用者，而旗标自己不指向任何文件。

## 本阶段不做

依赖账号/成员的属性语义（Person 的协作含义、权限）；评论；发布为站点。

==================================================
三十九、第二十五阶段：Database
=============

这是 Quire 与 Notion 差距最大的一层。

Database 不是 §九 块类型清单里的一行，它自带 model / storage / UI 三层，因此独立成阶段，阶段内再切里程碑。—— 2026-09-22 Track 3 **D0 只做决策与探针，未交付功能**：实体形状 ADR-0060、列定义 ADR-0061、值 ADR-0062、record 与 page 的关系 ADR-0063、视图定义 ADR-0064、Markdown 通道 ADR-0065。第一条性能红线已用 headless 探针证明通道存在（`core::database::window`：10 000 行 realize **31** 行，窗口 **6 806 B** 对全表 **2 259 800 B**，原始行 `benchmarks/results/2026-09-22-track3-probe.jsonl`），实现自 D1 起。

## 对象模型

database：一组 record + 一份 schema（列定义、视图定义）—— 实体形状定于 ADR-0060：自己的 `databases` 行，由一个新的 `Database` 块经 `blocks.db_ref` 指过去（同 ADR-0026 的 `page_ref`）；「整页数据库」就是首块是它的普通页，八个视图是同一个实体的八种 `layout`，不是八个块种类。列定义与视图定义分别见 ADR-0061 / ADR-0064 —— 2026-09-22 **D1 落了这一层的存储**：`databases` / `db_properties` / `db_records` / `db_values` / `db_value_items` / `db_views` 六张表（schema **v12–v15**，一步一语义单位）与 `core::database` 的对象模型；行只活在窗口里（ADR-0067），批量路径不许把这一层弄丢（ADR-0066）。**块仍未点亮**：ADR-0060 的六个接点整批留待画视图的那一刀（D3）。

record：一行，可以同时是一个 page（页面即行，这是 Notion 的核心而不是装饰）

view：同一份数据的一个投影（过滤 + 排序 + 分组 + 可见列 + 布局）

property：列，带类型

record 与 page 的关系必须可逆：删 record 与删页面的行为都要有明确定义，且都进 undo。—— 2026-09-22 契约定于 ADR-0063：record **拥有**它的 page（`db_records.page`，`UNIQUE(page)`，`ON DELETE CASCADE`）；标题只有一个家（有 page 在 `pages.title`，无 page 在 `db_values`，读的时候 `COALESCE`）；record 默认没有 page，第一次打开才建，建页与标题搬家是同一个 `Command` 的一批 change；删 record 是 `[DbValueDeleted…, DbRecordDeleted, PageDeleted?]` 一批（`Entry` 的 `apply`/`revert` 一起算，一次 Ctrl+Z）。**已知缺口**：侧边栏自己的删页从来没进过 undo，那条路上丢的 record 是丢的——ADR-0063 明写，不假装闭环。测试 2026-09-22 落在 D1（`tests/integration/storage_test.rs` 的 `database_layer`：两条删除路径 + record 与 page 的重建 + 批量路径），命令层那个「一次 Ctrl+Z」随 D3 的 `Command`。

## 属性类型

必做：title / text / number / select / multi-select / status / date / checkbox / url / email / phone / files / created time / last edited time —— 2026-09-22 存储形状定于 ADR-0061（列定义是 `db_properties` 行表，只有 select 的选项列表是行内 JSON）与 ADR-0062（值是 `db_values` 一行一列，`text`/`num`/`flag` 三列 + `db_value_items` 给列表型）—— 2026-09-22 D1 落了**存储**：每种形状都有往返测试，「空」= 没有行（不是空串、不是 0），select 的选项 config 原样存 JSON。逐类型的渲染与语义（选项名、日期格式、附件显示）仍是 D2。—— 2026-09-22 **D2 交付**（ADR-0068…ADR-0071）：14 种属性的**输入与渲染规则**落在 `core::database_property`（数字/日期/勾选的接受与拒绝逐条成文，url / email / phone **只提示不改写**，选项存 id 不存标签，附件显示名字，未知的选项 id 与已删的文件 id 各自显示自己），`created time` / `last edited time` 由 `db_records.created` / `.edited` 两列投影（v17，写路径盖章，永不写进 `db_values`），排序由 `SortSpec` 编译进 SQL 的 `ORDER BY`（数字按 `num` 排、日期按定宽文本排、空值显式排在最后，`EXPLAIN QUERY PLAN` 为证）。

降级处理：person —— 没有账号体系，退化为工作区内本地成员名单，纯字符串—— 2026-09-22 落于 ADR-0071：仍然没有成员表、没有成员 id、没有账号（ADR-0061 的折叠不动），名单由 `SqliteRepository::workspace_people()` 从值里现算（存储 kind 为 `person` 的列的去重非空值），改名就是改一个字符串。

需计算：formula / rollup / relation（含双向关系）

公式引擎的限制：纯词法 + 自写解释器，不引入 JS / WASM 运行时；表达式必须有限求值；relation 环检测在保存时做，不在渲染时做。—— 2026-09-22 **D6 交付 formula**（ADR-0082/0083 随刀）：引擎是 `core::database_formula` 的纯词法 + 递归下降 + 树遍历解释器（零新依赖），类型 number / text / boolean / date + Empty（Empty 传染，`text(x)` 是唯一显式转换，无隐式转换），函数七个 `if length round abs min max text`；四个求值预算是常量（tokens 2 048 / depth 32 / steps 10 000 / result 65 536）；表达式存列自己的 `config` JSON（ADR-0082，`PropertyConfigSet` 整文档替换、一步 undo），**值不入库、投影时现算**（ADR-0062/0039 的纪律）；环检测在保存时做（`would_cycle`，渲染时只有深度上限兜旧文档，画 `Error` 不挂）；公式列的排序/过滤被拒绝（要比较就得先算全列，正是红线禁止的事）。**rollup / relation 已交付**（2026-09-22 **D9**，ADR-0088/0089/0090）：Track 2 的引用基础设施（`reference.rs` / `backlinks.rs`）到 D9 仍未提交，所以按 ADR-0084 写死的形态单独落一刀，不另造轮子——relation 是列值为**目标库 `RecordId` 列表**的 `CellValue::Items`（ADR-0088，存 id 不存标题），一对关系是一次**对合**（involution）：配对的判据是「候选已声明 target 指回我」或「候选是自由列（没声明 target）」，一次 `Command::SetRelation` 把前向格与镜像格写成**一批 change**（一次 Ctrl+Z），所以**环在结构上不可表示**（不是渲染时检测，是存不下）；rollup 是列 config `{"relation","column","aggregate"}` 的六种聚合（None / Count / Sum / Min / Max / Average，复用 `database_formula` 的 `arith` 与 `extreme`），**值不入库、投影窗口时现算**（ADR-0062 的纪律），配置期拒绝跨库环，读路径对计算列返回空映射 ⇒ **嵌套 rollup 的深度上限为 0**（ADR-0089）。relation / rollup 列都**不是行内可编辑**的（选择走 picker）。**D9 只落状态层与文档**：`.slint` 的列类型菜单、record picker 弹层、rollup 三段配置器仍未接（D5 留下的欠账一并记在 REPORT_TRACK3 §D9），用户视角还够不着——同 D0 的诚实边界。—— 2026-09-23 **D10 清了这笔欠账**：三个入口都接到了 `.slint`：列类型菜单是 Columns 弹层的第二个面板（`db-columns-panel == 1`，列表由新组件 `DatabasePickList` 画，行是 Rust 推下来的 `DbPickRow`，把拒绝的话写在灰掉的行上而不是弹窗里）；relation 的编辑器是 `DatabaseRelationPopup`（一窗三态：target 库、back-pointer、按 needle 搜索的 record picker，选中的行画 tick）；rollup 的三段配置器是 `DatabaseRollupPopup`（一句三段话，每段开自己的 `DatabasePickList`，chooser 逐行走 `check_config`）。后两个是窗口级 `PopupWindow`，照 D10 既有的规矩从 `apply_scene_overlay` 打开；出口是 `close-on-click-outside` 与各自的 `*-closed` 回调，回调里 Rust 收回窗口标志（D10 收尾补的一处：Columns 弹层的类型面板关掉时要复位 panel/property/title，否则下次打开落在过期的菜单上）。**Escape 那一半只有画面证据**，键盘与开屏焦点要人手测（REPORT_TRACK3 §D10 的 6.1）。状态层同刀补了三处：**改类型**是新的 `Command::SetDatabasePropertyKind`（ADR-0062 的 `PropertyKindSet`，值不转换，计算列改走普通列会先给一句「得先定义」的提示，同 kind 的点击零 change 因此不花 undo 步）；**过滤算子按 kind 收敛**（`FilterOp::ops_for`，不该出现的算子在选择器里就没得选，而 `FilterValue::Missing` ⇄ `Json::Null` 那条「规则在、值没填」的语义在 `build_clause` 里被修成真的不加约束）；**搜索的计数与缓存键**（`layout_total` 以前无视 needle、`DbWindow` 以前没把 needle 当键，换词不重画）。像素这一刀说了四次「不」：`✓` `✕` `⋯` 三个码位不在 Segoe UI 的字体脸里（ADR-0092：要用户看的标记一律画，不用字形），已交付的三处文案随之改写。数据库块的 16 亮 + 16 暗场景进了 sweep 基线，rollup 的截图同时是 ADR-0089 的端到端证据（Total = 8 + 6 = 14）。仍没做的：真窗口里的手测（截图/点击自动化在本项目禁用，见 REPORT_TRACK3 §D9 的边界），以及 relation 的**双向扇出无上限**（10 000 条 fan-out 28.5 ms vs 9.1 ms 对照 = 3.12×，是产品决策不是 bug，等用户定）。ADR-0062 记下了一条：formula / rollup / relation **不存值**，投影时现算，`created time` / `last edited time` 的来源（当时 `pages`/`db_records` 都没有时间戳列）留 D2 出 ADR，不写进 `db_values`（否则就是 ADR-0039 禁止的双写）—— 2026-09-22 D2 交付：ADR-0068 给 `db_records` 加 `created` / `edited` 两列（v17，`YYYY-MM-DDTHH:MM` 本地墙钟，由 SQLite 的 `strftime` 在写路径盖章），读路径对这两种 kind 从不查 `db_values`。

## 视图

table → board → list → calendar → gallery → timeline → form → chart

顺序即实现顺序。chart 放最后，且不得为此引入图表库：先用现有绘制 primitive 做 bar / line / pie 三种。—— 2026-09-22 形状定于 ADR-0060（八种是同一个 `Database` 块的 `db_views.layout`，不是八个块种类）与 ADR-0064（视图定义持久化）。「+」插入菜单里那六行 muted 占位（`INSERT_ITEMS` 的 `Table view` / `Board` / `Gallery` / `List view` / `Calendar` / `Timeline`，`id = -1`）就是其中六种 layout，点亮它们 = 给这些行真 id，不是加块种类。2026-09-22 D1 落了**视图定义的存储**（`db_views`：名字 / layout / 顺序是列，规则是一份 JSON，ADR-0064），视图本身（渲染与切换器）仍未有，六行仍不可选。 —— 2026-09-22 **D3 交付 table**（ADR-0072…ADR-0075 随刀）：`DatabaseView` / `DatabaseCell` / `DatabaseSwitcher` 三个组件与 `BlockKind::Database` 块（`blocks.db_ref`，v18）点亮，窗口投影真的从 SQL 取行（`core::database::window` 算出 `LIMIT`/`OFFSET`，10 000 行只 realize 视口那一窗，行的 y 由 `db-row-start` 摆回滚动面的原位——「行是动态的」两条规则照 §三十七）；视图切换器是真实的 tab 条（今天每库一个视图，D5 加第二个视图只是加一行）；「+」菜单的 `Table view` 行点亮（真 id），其余五行仍 muted，board 起的七种打开时以自己的 layout 名明说「not in this build yet」。 —— 2026-09-22 **D5 交付 board / list / calendar / gallery / timeline / form 六种**（ADR-0078 / ADR-0079 随刀）：每个布局自己的「窗口单位」（board 的卡片槽位、gallery 的卡片行、calendar 的固定 6×7 格 + 每天至多 3 条折叠计数、timeline 的泳道 + 一次 min/max 定轴、「无日期不显示」是语句里的 is-not-empty 子句、form 不读行只建行），全部照「计数先算、窗口后开」；board 复用 D4 的 `groups` 键与 `GROUP BY`，calendar/timeline 的时间轴是视图文档的新键 `date`/`end`（零迁移），视图切换器「+」点亮（`AddDatabaseView` 一个 change，创建即切换，chart 仍以名字拒绝）；插入菜单其余四行仍 muted（ADR-0079）。 —— 2026-09-22 **D7 交付 chart**（第八种，随 ADR-0078 的窗口单位契约落地）：plot 画的是**聚合不是行**——一次 `GROUP BY`（复用 D4 的 `group_counts`，与 board 的列、分组表的组头是同一查询）给出的 (键, 计数) 序列，键数由选项有界的分组列封顶，10 000 行的库 realize **0** 行、画几个形状；三种形状 bar / line / pie 全用现有 primitive（bar 是等宽 Rectangle、line 是一条 viewbox 缩放的多段线、pie 是 Rust 端 κ 近似三次曲线的逐片 Path，无图表库），形状存视图文档的 `chart` 键（ADR-0074 的读改写，bar / line / pie 三键切换，切换只改形状不改数据）；无分组列时画「Pick a column to group by」，计数文案照实说；切换器「+」的第八行与 `db_add_view` 的 chart 拒绝随之点亮。

## 操作

filter / sort / group by / 视图内搜索 / 行内编辑 / 列宽与隐藏列 / 视图切换器；视图与 schema 一起持久化 —— 2026-09-22 **D3 交付行内编辑、列宽与隐藏列、视图切换器**（ADR-0072…ADR-0075）：行内编辑接了 title / text / number 的行内输入、checkbox 的整格点击、select / status 的格内选项列表（一个 `TextInput` 纪律照旧，值经列自己的 kind 解析，ADR-0069）；列宽是 permille、存进视图文档（ADR-0074 的原样透传），隐藏列走窗口级 Columns popup（title 列锁定，ADR-0063）；切换是会话态（ADR-0073）。—— 2026-09-22 **D4 交付 filter / sort / group by**（ADR-0076/0077）：过滤树与多键排序编译进窗口读的同一条语句（`storage::database_query` 出文本与绑定、`database_store` 执行，「不在 UI 侧过滤」是模块边界而不是纪律），计数先 `COUNT(*)` 后开窗，过滤 10 000 行剩 3 行就 realize 3 行；分组是**条目投影**——组头是条目不是行（窗口算术跑在 Σ(count+1) 上，组内行各自 LIMIT/OFFSET），组列表只对 option-bounded 的 checkbox / select / status 开放；过滤树整棵读不开时丢弃并在视图上**可见提示**，单条不可读的子句丢弃并计数（ADR-0064 的删列规则）；面板只编辑扁平子集（根 and/or + 子句级非），嵌套树照常过滤但拒绝被面板改写。 —— 2026-09-22 **D7 交付视图内搜索**（ADR-0087）：选路是**数据库自己的 SQL 谓词**（`INSTR(LOWER(expr), LOWER(?)) > 0` 的 OR，编译进窗口读的同一 `WHERE`，count / 组查询 / 组内切片同吃），不是 §二十 的 FTS5 索引——`search_pages`/`search_blocks` 只镜像页标题与块文本、不索引 `db_values`，挂进去就要为每格写入加一条维护路径、为每条批量路径加一条清理规则，还要背上「哪个写入者记得索引」的滞后边界；`INSTR` 是全扫（与 D4 的 contains 同价同界，LOWER 只折 ASCII），但**零副本零滞后**。搜索范围 = 视图可见的承载文本的列 + 标题（数字/勾选/选项 id/列表/计算列不搜，理由在 ADR）；needle 是**会话态**（ADR-0073 的规则用于一次提问），不进视图文档、导出带 `search: None`，头部计数槽即搜索框的开关。视图定义怎么存定于 ADR-0064：视图是 `db_views` 行（名字 / layout / 顺序是列），**规则**（过滤 + 排序 + 分组 + 可见列 + 列宽）是一份 JSON 文档，判据是本 ADR 与 ADR-0061/0062 共用的那一句「SQL 有东西要在它上面过滤吗」——列和值要在，视图规则不用。

linked database：引用另一个库的某个视图，不复制数据 —— 2026-09-22 **D7 交付**（ADR-0085）：形态是**同一个 Database 块 + 同一根 `db_ref`**（不是新 kind、不是第二列、零迁移）——`Command::LinkDatabase` 把一行变成指向**已存在**实体的 Database 块，读与写本来就全部经 `db_ref` 解析，所以「读源库的数据与视图定义、写回也写源库」是结构成立的（世上只有一份数据，没有可漂移的副本）；SPEC 草案里的 `(db, view)` 中 view 一半按 ADR-0073 归会话态，存储只指库。入口是 slash/插入菜单的 `Linked view` 行 → 数据库 picker → 一批落地；源实体死（只有撤销建库块一条路）走 ADR-0060 既有的 `(deleted database)` 一行。

数据库模板：新建 record 时的预填 —— 2026-09-22 **D7 交付**（ADR-0086）：模板是 `databases` 行上的一个 JSON 文档（v20，`''` = 无模板），每个值都是 `CellValue` 存储形状的**原样副本**（不引入第二套内容格式，与 Track 1 页面模板同一条纪律；两边没有需要仲裁的共享形状，报告已说明）；行槽的 T 把一行现存值存成模板（一个 change 一步 undo），`db_add_record` / `db_form_submit` 把模板格作为普通 `SetDatabaseCell` 并入**建行同批**（一次 Ctrl+Z 连行带预填一起撤）；公式列与两个时间戳不进模板（无存储值可拷，ADR-0062/0068）。

**Markdown 通道（§二十六 是内容通道）定于 ADR-0065**：database 导出成它**当前显示的那个视图**的 GFM 表格（title 列在最前，一列一个可见属性，一行一条 record，按视图的顺序与成员，即过滤排序照做）；页-backed 的 record 标题写成 `[title](quire://page/<id>)`。**不写标记行**（`<!-- quire:toc -->` 那种先例不适用：目录没有别的表示，表格本身就是真表示）。导入侧不改：管道行回来是段落（ADR-0031 为简单表格定的同一条），所以一次导出/导入丢掉 schema、类型、record 身份——这是决定，不是遗漏。代价：`export_page(blocks)` 拿不到 record，导出时由调用方把渲染好的行传进来，不让导出层学会开数据库。

## 性能红线

Database 是本规格里唯一会自然长出「大量行 × 大量属性」的功能，§二十二 / §二十三 的规则在这里最容易破：

* 10 000 行的库不得全量 realize；视图先算可见窗口再取行 —— 2026-09-22 **通道已证明存在**（D0，ADR-0060）：`core::database::window` 先算 `[start, end)` 再取行，`RowWindow::fetch()` 就是那次查询的 `LIMIT`/`OFFSET`；10 000 行、32 px 行高、720 px 视口、8 行 overscan 时 realize **31** 行（窗口 0..31；滚到中间 39 行、底部 31 行），窗口那 31 行占 **6 806 B** 堆，全表 10 000 行占 **2 259 800 B**（332×），只要 id 的 `Vec<u64>` 是 80 000 B。可重复：`cargo test --lib database::`（断言）与 `cargo test --release --lib -- --ignored --nocapture a_window_costs`（数字），原始行 `benchmarks/results/2026-09-22-track3-probe.jsonl`。2026-09-22 **D1 补上了 SQL 那一半**：窗口真的作为 `LIMIT`/`OFFSET` 执行（`storage::database_store::window_rows`），10 000 条 record 落库后一次窗口读拿回 31 行（10 000 行的对照也量了，见 `benchmarks/results/2026-09-22-track3-d1-window.jsonl`）。**仍然只证明了投影与查询，没证明帧**：没有 `.slint` 视图，没跑过 `bench.ps1`。
* filter / sort 在 SQL 侧完成，不在 UI 侧过滤 —— 2026-09-22 **形状已交付**（D4，ADR-0076/0077）：过滤与排序编译进窗口读的语句，计数先 `COUNT(*)` 后开窗（ADR-0062 的形状——`num REAL` 可索引、日期是定宽 ISO 文本——在这里兑现成比较发生在 SQLite 自己的类型系统里）；**耗时对照仍是统一测试的数字**（「10 000 行的库加一个过滤条件的窗口读」对「取回 10 000 行再在内存里过滤」，量法记在 REPORT_TRACK3 §D4）。
* formula / rollup 必须可增量重算，禁止每次输入全库重算 —— 2026-09-22 **D6 交付 formula 半边**（ADR-0083）：求值只发生在投影窗口（六布局共用的 `db_table_rows` → `db_paint_formulas`），求值集 = realize 行 × 可见公式列，**没有遍历全表的求值路径**；依赖是本行的（引擎回调无 record 参数——公式在结构上引用不到别的行），改一格后值可能变化的格 ⊆ {该行} × 依赖闭包；`db_formula_evals` 计数器钉住量法——统一测试要量「改一格后 evals 增量 = 窗口行数 × 可见公式列数，与 COUNT(*) 无关」与「值实际变化的格 ⊆ 依赖闭包」两个数。没有跨刷新的值缓存：失效要覆盖 undo/redo/批量替换的每条写路径，漏一条就是画陈旧值，比确定性的微秒级重复求值更糟（ADR-0083）。导出是显式全视图产物、按行现算，不属输入红线（同 ADR-0083 的边界）。rollup 等 relation（ADR-0084），**2026-09-22 D9 随 relation 落地并继承同一契约**（ADR-0089）：求值集 = realize 行 × 可见 rollup 列，经 `values_of` 对被引 record 一次批量取值再折叠，**没有遍历被引表的路径**；探针量化在 fan-out 10 000（整表被引）时窗口读 **28.5 ms** 对对照 **9.1 ms**（**3.12×**，窗口为活标题 + 值两遍付账，见 `benchmarks/results/2026-09-22-m14-relation-rollup.jsonl`），契约是「被 **fan-out** 封顶、与**表大小**无关」，而 fan-out 今天**无上限**（整表被引时即整表的值都要读）——这是留开的产品决定，缺口照实记在 REPORT_TRACK3 §D9。
* 数字进 docs/PERFORMANCE.md：10 000 行的 RAM、切换视图耗时、打开公式编辑器的耗时 —— 仍欠着（没有 UI 臂就进不了 PERFORMANCE.md 的 §Method）。D1 的建库与窗口读数落在 `benchmarks/results/`，随 D3 / D8 与真窗口读数一起收口。探针的窗口数字落在 `benchmarks/results/2026-09-22-track3-probe.jsonl`（进程 private 2.0 → 5.1 MB，即把 10 000 行真的拿进内存要 ≈3.1 MB，而窗口只要 6.8 KB），三个正式数字随 D3 / D8 进 PERFORMANCE.md。 —— 2026-09-22 **D8 收口**：三个数字进了 `docs/PERFORMANCE.md` 的 `## M14`（原始行 `benchmarks/results/2026-09-22-track3-d8.jsonl`）：① 10 000 行的库开在窗口上只占**几 KB** 行对象（31 行，与 100 行库同价），全量 realize 才要 1.16–2.26 MB（≈330×），是永不发生的那一支；② 切到视图顶部 = 解码 + `COUNT(*)` + 一次窗口读 ≈ **0.35–1.0 ms**，但 `OFFSET` 走到表尾要 **16–25 ms**（cursor 取窗实测 212–356 µs，约 80× 之下，是退役它的现成路子），分组视图的 `GROUP BY` 再 +5–18 ms；③ 打开公式编辑器 = 解析一条嵌套式子 **1.6–3.4 µs**、单格求值 68–129 ns，把 10 000 行全重算是窗口重算的 **108×** 且投影无路径去做（ADR-0083）。仍**未测帧**：Slint 重画 31 个 delegate、日历 42 格、chart 构 path 的墙钟，要真窗口 + `bench.ps1` 的 RAM/像素臂才量得到，headless 探针替不了它（与 D0 同一条界限）。

## 排期前提

本阶段在 §三十七 批次 B（table / toggle / columns）与 §四十 的引用基础设施之后开始，否则简单表格和 relation 会各造一遍轮子。

==================================================
四十、第二十六阶段：引用、提及与反向链接
=============

@page mention：§十 的 Inline Model 新增一种 span（存目标 page id，不存标题）。输入 @ 弹页面选择器，复用 §十五 slash 弹窗的第三种模式（ADR-0026 已验证这条弹窗可复用）。

—— 2026-09-22 已交付，ADR-0050（存储形态与 Markdown 往返语法）。span 复用 `marks` 表唯一那列载荷（`url = "quire://page/<id>"`，`kind = "mention"`），**不新增列**。chip 的标题由**投影层**向 workspace 现问（`MentionTitles`），所以改名自然跟随、没有任何字符被重写——代价的另一面是删页时也没人去改引用，退化因此是投影层的职责（见下面「页面别名」）。选择器是 slash 弹窗的**第四种模式**（`slash-pick-mention`）：第一行是日期，其余是 workspace 的页面；应用时发一条 `InsertReference`，因为 `exec_all` 对批内每条命令都按同一份 pre-state 计划，「替换 `@` 之后的过滤文字」与「落一颗 mark」必须是一步（否则后者的按旧文本长度 clamp 会把它切错），顺带也是**一步撤销**。点击 chip 走 M8 就有的 `quire://page/<id>` → `open-link` 路径。悬空（目标页已删）时 chip 读作 `(deleted page)` 并灰化。

@date：落 date 型 inline span。

—— 2026-09-22 已交付，ADR-0050。日期**存自己的 ISO 文本**（同一列载荷 `url`），不存"今天的编号"：格式的唯一定义在 `core/date.rs`（`is_iso_date` / `to_iso` / `today_iso`），导入侧与选择器写盘共用它，所以库里不会出现两种写法。渲染成独立的 `clock` chip，与 mention 在视觉上分开——日期不是指向页面的引用，它没有地址。

反向链接区：页面底部列出所有引用本页的块。派生数据，不双写入库。

—— 2026-09-22 已交付，ADR-0051。**不加表、不加列、不挂 FTS5**：migration 16 只加两条索引（`marks(kind, url)` 与 `blocks(page_ref)`），查询在 `storage/backlinks.rs`，每次投影现算（`refresh_backlinks`，由 `reproject_blocks` 调用）。面板**折叠 5 行 / 展开 50 行**，超出的部分由一行总量交代，所以被引 200 次也不会把正文挤出屏幕；按来源页分组，组头只在每组第一行画。点一行跳到来源块，来源在别的页就先开那一页——和文本里的 `quire://block/` 链接走同一个 `jump_to_block`。

反向链接索引与 §二十 的搜索索引一起增量维护，不得每次打开页面全库扫描。

—— 2026-09-22 达成，但**路线与原文不同**，见 ADR-0051：这条 ADR 撤掉了本 track 早先的"挂进 FTS5 增量路径"草案，改为"两条索引 + 派生投影"。挂进搜索索引意味着多一份派生数据、也就多一条必须维护它的写入路径（删一条 mention 要从 `search_blocks.content` 里摘掉 `__backlink:` token，而摘的动作会把用户正文一起重写），而索引由 SQLite 从磁盘上已有的行建出来，**没有可漂移的副本**。"不得全库扫描"这句**有对照数字**：同一进程、同一个库（1 200 页 / 100 200 条 mark）里把 `idx_marks_reference` drop 掉再跑同一条查询，折叠读从 **78 µs 变成 6 068 µs**（`docs/PERFORMANCE.md` "T2 · the backlink panel is one index seek"），页面打开本身 +0.06 ms（折叠）/ +0.07 ms（展开），对 §二十二 的 50 ms 预算。

页面别名：因为引用存的是 ID，重命名后所有引用自然显示新标题。

—— 2026-09-22 已交付，ADR-0051「后果」小节。**这一条不需要新功能，需要的是证明**，三处各有测试钉住：chip（`a_mention_run_reads_the_live_title_and_degrades_when_the_page_is_gone`）、反向链接面板的分组标题（`the_backlink_panel_groups_by_page_and_names_each_page_as_it_is_called_now`）、Markdown 导出（`an_export_that_knows_the_workspace_names_each_page_as_it_is_called_now`，导出侧走 `export_page_with(..., &|id| ws.title_of(id))`）。「引用存 id」的三种代价也定义并钉住了：目标页**被删** → chip 与块级引用行都读 `(deleted page)`（`deleting_a_page_degrades_the_chip_that_named_it`）；目标 id **指向本库不存在的页**（手改过库 / 旧备份）走的是同一条代码路径——都是"workspace 报不出这个名字"，所以退化是同一句话，不另造一套。顺带修了一个真 bug：`delete_page` 删掉**非当前页**之后没有重投影，屏幕上的 chip 会一直显示旧标题（这正是"可见退化"要挡的那类失败：没有任何东西会去纠正它）。目标页被**移动**到别的父页不属于退化——id 没变，所以引用照旧解析，`renaming_moving_and_losing_the_page_a_reference_points_at` 把改名、移父、id 落空三件事放在一条测试里按顺序走了一遍，并断言那颗 chip 所在的块在三种情况下**逐字节未变**（引用只存 id，改的是一个页面，不是三千个引用它的块）。

synced block 建在这一层之上：一个块被多处引用，编辑任意一处全部生效。

—— 2026-09-22 已交付，ADR-0052。**一块内容只有一份**：`Synced`（kind 24）自己不持有文本，
`blocks.sync_ref`（migration 19）指向源块，行画的是源块的 text 与 marks，投影时解析、不第二次写
（`sync_target`）。编辑绑到源块（`content_of` / 行的 `content-id`），所以"两处同时变"真的只写一处，
一步 Ctrl+Z 撤掉。四个语义都在 ADR 里落了字：删镜像只删这一行（无外键、无级联）、删源让镜像可见退化成
`(deleted source)` 并变只读、undo 无需合并、**环检测在写入那一刻做不在渲染时做**（`sync_would_cycle`，
有上界；`sync_target` 也带 `SYNC_RESOLVE_MAX`，旧备份里的环是"画错"而不是"挂住"）。
只同步**一个块**不同步整棵子树 —— 那会让行数变成动态的，§三十七 那两处附加改动（真删子树、row→model）
在这一刀被判定为用不到，不是被漏掉。Markdown **导出摊平**（镜像导成源块那一行，照 ADR-0032 columns
的先例），**导入有意不认新语法** —— §二十六 把 Markdown 定成内容通道不是保真格式，block id 跨库没有意义，
造个记号只会产出"导进来立刻失去源"的块。六接点：`BlockKind::Synced` / kind 24 / 导出摊平+导入按名字
回读 / `TURN_INTO_ITEMS = SLASH_ITEMS` / slash 与 "+" 两个菜单 / 场景 `synced`、`synced-source-gone`
及各自的 `dark-` 臂。**未验证**：真键盘输入、源与镜像同时在屏时两个输入框的焦点争用、真点一次跳转。

## 四十一、笔记与任务（组织器）

与文档并列的第二个顶层区域：`UIState.active-area` 只有两个值 —— 页面与组织器，切到组织器时
`AppShell` 用 `OrganizerArea` 取代 `Editor`，页面本身不被替换，切回来还在原处。顶栏的
「设置」下多两行 **笔记 / 任务**（`org-open-requested(0|1)`），抽屉侧栏是同一对行，手机另在
拇指条上加了第七个入口 —— 都是"去一个地方"，不是命令，所以没有快捷键提示。 —— 2026-09-24
**交付**（ADR-0098…ADR-0104 随刀）。

数据模型在 `quire-core`：`Note` / `TaskList` / `Task` / `Subtask`（migration v24–v26 建
`notes` / `task_lists` / `tasks`，后者带 `(list,ord)` / `(done)` / `(due)` 三个索引），落库走
与页面同一套行级 change（9 个 `Change::Note*` / `TaskList*` / `Task*`），不是文档级替换
（ADR-0099）。**组织器有自己的一摞 undo**（`core::ORGANIZER_STACK`，一个页号永不被任何页占用，
ADR-0100）：在区域里按一次撤销只动区域，页面那边的撤销也只动页面，两边的 `edited` 时间戳各记各的。

笔记是标题 / 正文 / 标签 / 置顶；任务是标题 / 清单 / 优先级（无低中高）/ 截止日期 / 重复
（不重复·每天·工作日·每周·每月）/ 标签 / 备注 / 子任务清单。视图是 收集箱 / 今天 / 近七天 /
全部 / 已完成 五个**谓词**（同一个 catalog 上的五种问法，不是五份数据），排序是 添加顺序 /
优先级 / 截止日期，两个页签各有自己的搜索框（一个 needle 一次只筛一个页签）。清单是"名字里
没有对应 `task_lists` 行的那些任务"，所以删掉一个清单**在同一批 change 里**把它的任务移进收集箱
（`Command::DeleteTaskList{list, moved}`），一次撤销把清单和它原来的任务一起拿回来（ADR-0101）。

**本 shell 的这一份是单栏**（桌面把同一个组件挂成左列表右详情）：列表占满整屏，点一行翻到那一行
自己的样子，返回键再用一次选择回调（`org-note-selected(-1)`）取消选中翻回列表。行高 44 dp
（触屏最小可点面积），整份文件里没有 hover 才出现的可点物、没有一处键盘提示；退格/删除落在
notice band 这一条常驻带上（本 shell 没有三秒浮层），文案与桌面同一处产生（ADR-0103）。

**未验证**：真机上的长按与 44 dp 命中、中文输入法在组织器多行框里的行为、两端互相同步的实机往返
（`SNAPSHOT_VERSION` 升到 2，两个 shell 必须一起更新）。
