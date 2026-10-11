# InSync 最终交付报告

日期：2026-10-11　分支：`feat/insync`（自 `catdog` = `origin/master@65425b75` 切出）

## 1. 实际完成的产品功能

**桌宠（A 组验收）— 已完成**

- 原创像素人物「Momo」（程序化生成、双配色变体 momo / momo-mint，48×64 逻辑分辨率 4x 输出）；
  新增第二人物「Nana」（按使用者真实照片 redesign：深棕双麻花辫 + 中分空气刘海 +
  玫瑰粉无袖上衣 + 斜挎包带，绘制按 `Style` 结构旗标分支）。角色可在设置面板切换，
  桌宠与面板头像/空态插图同步跟随；旧配置无该字段时默认 Momo 照常启动。
- 透明置顶无边框窗口（macOS NSPanel / Windows UpdateLayeredWindow），可拖动、位置持久化、50–400% 缩放、多屏坐标钳制、点击穿透开关。
- 13 个动画：idle / blink / wave / walk / sleep / work / stretch / react / poke_react / message_react / invite_react / celebrate / rest（`tools/insync-sprites/validate_sprites.py` 强制帧数/边距/调色板/循环一致性）。
- 行为状态机：优先级仲裁（BASE/LIFE/REACTION/INTERACTION）、反应节流、随机生活动作（种子化可测）、点击反应、戳戳反应、消息/留言/邀请/庆祝反应。
- 托盘菜单（打开面板 / 戳戳伴侣 / 隐藏显示 / 退出）与干净退出。

**双人互动（B 组）— 已完成（服务端 + 引擎层经集成测试验证）**

- 注册/登录（Argon2id）、邀请码绑定、解除绑定、token 轮换与撤销。
- 实时消息（WS 推送 + REST 补拉 + seq 游标 + client_id 幂等去重 + 未读徽标 + 已读回执）。
- 异步留言（未读/已读、发送方可删）。
- 戳戳（本地先行动画、服务端 2s 限频、过期丢弃不重放）。
- 共同专注：邀请→接受→共享 end_at 倒计时→暂停/恢复（自动顺延）→提前结束/自然完成；revision 幂等；双方状态收敛。
- 共享待办/便签/纪念日（便签 revision 乐观锁，冲突返回当前内容并通知用户，不静默覆盖）。
- 在线状态（心跳、away/dnd、专注中标记）与断线重连（指数退避 1s→60s、日志补拉、`EVENTS_LAGGED` 重同步）。

**日常功能（C 组）— 已完成**

- 时钟（12/24h、日期星期）、本地闹钟（每日/星期/一次性，jiff 处理 DST，错过 10 分钟内补打一次）、专注计时、个人待办、共享待办、个人便签、共享便签、个人+共享纪念日（倒计时天数）、统一通知中心（同 key 节流 3s、勿扰时段、闹钟穿透勿扰、可选声音、历史 30 条）。
- 提醒音为程序化生成的 0.9s 双音铃声（C6→G5），`tools/insync-audio/generate_alarm.py` 用纯 stdlib 的确定性浮点实现合成（不调用跨平台可能差 1 ULP 的 libm `sin`/`exp`），因此 CI 可做字节级防漂移；铃声以 `include_bytes!` 内嵌，首次使用时写入私有数据根，并在启动时 `Prepare`（`bongocat-audio` 只播放已解码片段，否则 `Play` 会静默失败）。

**数据与安全（D 组）— 已完成（服务端强制）**

- 本地优先：桌宠、时钟、闹钟、个人数据、设置离线可用；共享数据缓存保留。
- 服务端逐请求鉴权（`require_active_relation`）；解绑即时撤销访问并广播清理本地共享缓存；非成员/伪造 token/越权 ID 均在集成测试中验证 401/403。
- 日志只输出单行启动信息；消息正文不进任何日志（有专项测试）。

**工程交付（E 组）— 大部分完成（见 §5/§7 的明确缺口）**

- 六个 InSync crate 合计 88 个自动化测试全部通过（见 §4）。
- macOS aarch64 实机构建 + 冒烟通过；Windows x86_64 交叉 clippy 检查通过，构建交给 CI。
- 已推送到 `origin/feat/insync`，`.github/workflows/insync-verify.yml` 实际运行并逐项修好触发与构建问题（见 §7）。
- 独立版本号与发布通道（ADR-0092）：InSync 从 0.1.0 起自成一套版本，`insync-v*` 标签由
  `.github/workflows/insync-release.yml` 发布「该提交上 `insync-verify` 已通过的产物」，
  四个平台资产 + `SHA256SUMS` + changelog 正文；仍是裸二进制（无安装包/签名，§8.4）。
- Dockerfile + docker-compose + 部署/备份文档完成。

## 2. 最终技术架构

```
insync-app (GPUI + 桌宠窗口 + 托盘 + 闹钟线程)
 ├─ insync-sprite      精灵库/播放器/行为状态机（纯逻辑）
 ├─ insync-pet-window  macOS NSPanel / Windows 分层窗口（unsafe 仅限此 crate）
 ├─ insync-client      REST + WS + outbox 同步引擎（独立线程 + 自有 tokio runtime）
 └─ insync-core        领域模型 / 事件协议 v1 / 幂等与离线语义（server/client 共享）
        ⇅ HTTPS / WSS
insync-server (axum) ── SQLite(WAL)：users/sessions/relations/messages/letters/
                        pokes/focus/todos/notes/anniversaries/events/presence
```

细节见 `docs/insync/technical-design.md`；协议见 `docs/insync/protocol.md`；
决策见 `docs/adr/0089/0090/0091/0092`。

## 3. BongoCat 中保留、修改和删除的模块

- **保留未动**：全部 bongocat-* crate 源码、Live2D 栈、打包/更新流水线。
  `crates/bongocat-*` 下没有任何源文件在本分支被修改。
- **有一处共享依赖声明改动（如实记录）**：workspace `Cargo.toml` 的 `rodio`
  由 `features = ["playback", "flac"]` 变为 `["playback", "flac", "wav"]`，
  `Cargo.lock` 随之增加 symphonia 的 WAV/PCM 两个 crate。原因是
  `bongocat-audio` 的解码器原本只认 FLAC，而 InSync 的提醒音需要一个可确定性
  重建的短音频；BongoCat 自身播放的是同一批 FLAC 文件，行为不变。理由与取舍
  记在 ADR-0089「Audio reuse and its one enabled codec」。
  **lock 陷阱（如实记录）**：本仓库的 `Cargo.lock` 刻意把 Windows target 的
  `windows` 依赖边固定在单一版本（AGENTS.md §6.1）。`cargo update -p <crate>`
  会连同这些边一起按解析器偏好改回更旧的版本；本次已在提交前把 16 条
  `windows*` 依赖边恢复为原有版本，并用
  `cargo tree --locked --target x86_64-pc-windows-msvc -p bongocat-platform`
  复核（结果仍为单一 `windows v0.62.2`）。最终 `Cargo.lock` 的改动全部是
  新增行，无删除。
- **复用**：`bongocat-storage`（原子私有写）、`bongocat-audio`（rodio 播放）、
  `gpui-kit`、`tray-icon/muda` 依赖、JSON store 设计模式（照 bongocat-config
  的 schema 门 + 备份 + 隔离）。
- **新增**：六个 `insync-*` crate + `tools/insync-sprites/` +
  `tools/insync-audio/` + `resources/insync/` +
  `.github/workflows/insync-verify.yml`。
- **未删除**：无。未删除任何 BongoCat 功能或文件。

## 4. 实际执行的自动化测试及结果

| 套件 | 数量 | 结果 |
| --- | --- | --- |
| insync-core（协议/专注状态机/outbox/去重/缓存） | 28 | ✅ |
| insync-server 单元（DB 迁移/argon2/token） | 6 | ✅ |
| insync-server 集成（双用户全流程 + 安全矩阵） | 13 | ✅ |
| insync-sprite（库加载/帧计算/优先级/行为） | 13 | ✅ |
| insync-client（错误映射 1 + 引擎级双端集成 4） | 5 | ✅ |
| insync-app（store 6 + clock 7 + notify 3 + 铃声解码 1 + hub 5 + palette 3 + 素材内嵌 2） | 27 | ✅（不含 GUI 自动化） |
| bongocat-audio（回归：启用 wav feature 后原有测试） | 15（+1 ignored） | ✅ |
| 素材校验 `validate_sprites.py` | 26 sheet | ✅ |
| 素材校验 `validate_alarm.py`（格式/电平/削波/音高） | 1 WAV | ✅ |
| BongoCat 工具契约测试 `tools/tests/` | 122 | ✅（含 product version 契约、InSync 版本/发布契约 `test_insync_release.py` 20 项与 workflow 校验；其中 20 项由 `insync-verify` 的 `contracts` 任务在每次推送重跑，全套在 PR 触发的 `verify.yml` 上跑） |

命令：`cargo test --locked -p insync-core -p insync-server -p insync-client -p insync-sprite
-p insync-pet-window -p insync-app -p bongocat-audio`。六个 InSync crate 现在合计 **92**
个测试（角色工作把 store 从 4 提到 6、新增素材内嵌 2 个，`insync-app` 从 23 提到 27），
在 CI 的 macOS 与 Windows 任务上分别真机跑通；`bongocat-audio` 另有 15（+1 ignored）
在 Linux 任务上跑通。
另有 `cargo clippy --locked <六个 insync crate> -p bongocat-audio --all-targets -- -D warnings`
全绿（pet-window 另在 `x86_64-pc-windows-msvc` target 上全绿），以及
`cargo fmt --all -- --check` 与 `python3 -m unittest discover -s tools/tests` 122 项全绿。
自 2026-10-11 起，这些命令不再在本机执行（见 §8 第 9 条与 goal 文档的验证策略），
改由 `insync-verify` 工作流在三个平台上运行；本轮结果为 run 38119777698 全绿。
角色工作那轮（run 38149755002）`cargo fmt --check` 抓到 6 处格式 diff（本地不跑
fmt 的已知代价），修复后由 run 38149994327 复跑；素材与契约脚本
（`validate_sprites.py`、`generate_ui_images.py --check`、`test_insync_release` 20 项、
`test_release_changelog_contract` 12 项）仍在本机跑——它们不构建 Rust，不占 target。

## 5. macOS / Windows 真实验证情况

- **macOS（实机，Apple Silicon）**：`cargo build -p insync-app` 成功；应用真实
  启动并保持稳定（35s 冒烟：进程存活、SIGTERM 干净退出、RSS 118MB 平稳无
  增长、CPU ~6–8%，debug 构建）。NSPanel 创建、托盘、hub 窗口创建均走真实
  AppKit 路径。**限制**：测试环境的显示器输出为全黑（截屏捕获成功但内容全
  黑），无法目检动画外观；已用精灵 contact-sheet（`target/insync-preview/`）
  人工核验帧绘制正确。Windows 仅交叉静态检查，未实机运行。
- **Windows**：全部六个 InSync crate 已通过 **Windows 目标交叉编译验证**：
  `cargo xwin clippy --target x86_64-pc-windows-msvc -p insync-core -p
  insync-server -p insync-client -p insync-sprite -p insync-pet-window -p
  insync-app --all-targets -- -D warnings` 全绿（含 gpui-kit、tray-icon、
  muda、reqwest/rustls、ring 的 Windows 路径），server release check 亦通过。
  环境：`cargo install cargo-xwin` + `brew install llvm`（提供 clang-cl /
  llvm-lib），本机 macOS 交叉检查，不做链接。**Windows 实机运行仍未验证**
  （分层窗口代码未经真实 Win32 环境执行，见 §7）。
- **macOS release 二进制（CI 产物，实机）**：安装 CI 于 `326408e1`（run
  38108110093）产出的 `insync-macos-aarch64` release 二进制后实测：应用启动
  后 RSS 102MB → 稳定 ~90MB（77s 无增长），SIGTERM 1s 内干净退出，stdout/
  stderr 均为空；删除数据根下 `assets/` 后重启，chime 重新落盘为 0600 且与
  已提交的 `resources/insync/sounds/alarm.wav` 逐字节一致（sha256
  `a7177739…`）。服务器二进制 `/health` 返回 200，`/v1/auth/register` 返回
  201（此前在本机 debug 构建上完成过完整注册→登录→邀请→接受→发消息→对方
  拉取的端到端）。**未验证**：声音是否真实从扬声器播出（本环境无音频输出
  设备可核验）。
- **人工试用（使用者本人，安装版）**：试用暴露了三个自动化验证覆盖不到的
  缺陷，均已定位、修复并复验（详见 §5.1、§5.2）。

### 5.1 人工试用暴露的三个缺陷（已修复）

| 现象 | 根因 | 修复与验证 |
| --- | --- | --- |
| 托盘“打开面板”点击无反应 | 见下方两条 | `hub::show_hub` / `toggle_hub` 拆分 |
| 桌宠背后是一块实色矩形 | NSPanel 只设了 `setOpaque(false)`，未清 `backgroundColor` | macOS 后端补 `setBackgroundColor(clearColor)` |
| 面板“有点丑” | 视图硬编码奶油底 + 组件仍用默认灰色主题 | `palette` 接管 `Theme`（§5.2） |

**面板“打开”无反应**是两条原因叠加，都只有在真实桌面上才会出现：

1. **窗口被创建但从未提升**。InSync 是 accessory（无 Dock 图标）应用，自身从不
   成为活跃应用；未激活应用的窗口会被 macOS 排在所有其它应用窗口之后。窗口服务器
   的 z-order 实测（`CGWindowListCopyWindowInfo`）显示面板位于 Zen/VS Code 等所有
   窗口之后，而浮动层级的桌宠（layer 3）盖在普通窗口（layer 0）之上——用户因此
   只看得见桌宠。用 System Events 激活该应用后，同一区域 95.3 万像素立即变为面板
   内容，证实窗口本身存在且完好。
2. **菜单项语义是 toggle 而不是 open**。应用启动时已经创建过面板，于是第一次点击
   实际上**关闭**了那个看不见的窗口：点击前后对比窗口列表，面板窗口（`#94892`）
   在点击后从屏幕上消失。

修复：`show_hub`（启动路径与菜单项）总是打开并提升窗口、激活应用；桌宠右键为
「已在最前则关闭，否则提升」——被埋在其它窗口下的面板不会再被这个手势吞掉；用户
用窗口关闭按钮关掉面板后残留的死句柄会被丢弃并重建窗口（修复前该分支只会对着死
窗口做无用操作）。托盘安装失败也会写出原因而不是被 `.ok()` 吞掉。

验证（全部针对安装版实测）：窗口 z-order 中面板 index 22 排在 Zen 的 23 之前；
真实点击菜单项——面板已打开时**不再关闭**，用关闭按钮关掉后再点会新建窗口并置前；
真实右键（合成 `CGEvent` 右键事件）两条分支都正确；回归测试
`insync-app::hub::tests::*`（3 个，覆盖菜单项永不关闭、右键只关“在最前”的面板、
无活窗口时重建）。

**桌宠背景不透明**：`setOpaque(false)` 只声明「本窗口不是不透明」，AppKit 仍用
`backgroundColor`（默认不透明的 `windowBackgroundColor`）填满整个 frame。像素
证据：修复前窗口顶部 16 行采样全部是纯灰 `231,231,231`，而同高度的窗口外是桌面/
窗口内容（0/16 匹配）；修复后窗口内外的同一背景逐行一致（16/16），同时角色本身
仍正常绘制。精灵帧本身是透明的（约 64% 像素 alpha=0，无半透明像素），因此实色
矩形只能来自窗口。修复方式与 `bongocat-overlay` 已长期验证的配方一致。

### 5.2 面板视觉重做（“面板有点丑”）

第三条反馈是纯粹的观感问题，没有崩溃也没有错误日志。定位到的根因是**两套主题
各画各的**：面板视图自己硬编码了奶油色背景，而 gpui-kit 组件仍读默认主题的中性
灰色令牌，于是暖色底上浮着一层灰色控件；更糟的是调色板把 `button` 令牌设成了
白色，使默认 variant 的主操作按钮退化成一块看不见的浅色片。

修复：

- `insync-app::palette` 接管 gpui-kit `Theme`，把桌宠精灵的取色（奶油、暖褐、
  玫瑰、蜜桃）写成产品调色板，浅色 `cream` 与深色 `cocoa` 两套**各自调校**
  （浅色系直接翻转得到的深色里玫瑰会变成荧光色）。面板内所有颜色因此来自
  `ActiveTheme`，视图不再持有颜色。`Theme::update` 的 `reconcile` 会把 `colors`
  的改动同步进组件令牌表，所以「让主按钮变成玫瑰色」只需改一处。
- 结构上重画了页头（吉祥物头像 + 配对状态）、8 个带图标的 tab 磁贴、以及
  「卡片 / 气泡 / 空态插图」的内容骨架；空态与页头使用从精灵图裁切出的静态图，
  不另画一套角色。文案沿用 `i18n`（新增约 50 个键，中英齐全，并有测试扫描
  面板里出现的每个文案键，缺一个就失败）。
- 两条只有真实窗口才会暴露的契约：面板窗口订阅自身 appearance 跟随系统深浅色；
  面板停留分区写入 `window-state.json`（`hub_tab`，`Option` + `serde(default)`，
  旧文件缺少该字段照常读取并落在默认分区）。

验证（本环境无视觉可用的模型，因此全部用测量代替目检）：

| 项 | 方法 | 结果 |
| --- | --- | --- |
| 8 个分区都渲染正确 | 8 次真实窗口截屏，驱动真实「回到上次分区」路径 | 选中磁贴与分区一致；`stray_edge=0`（无内容被裁切） |
| 页头 / 磁贴 / 空态真的画出来了 | 在截屏上统计色块与几何 | 页头为 muted 底 + 吉祥物蜜桃 `#f4ceaa`/玫瑰 `#ed92a4` 色块；4×2 磁贴网格，选中磁贴 x=203..292 为玫瑰；空态插图为蜜桃色块（62×120pt） |
| 主操作可辨认 | 统计主按钮区域填充占比 | 玫瑰占 33.6%（修复前为白色，等同隐形） |
| 文字可读 | 27 条 WCAG 对比度断言（`palette.rs` 单元测试）+ 截屏实测 | 正文配对 ≥4.5:1；暗色实测 6.9–8.5:1 |
| 小窗口不挤 | 最小 360×500pt 截屏的列内文字边界 | 磁贴文字最宽 120..152 落在列 98..177 内，无裁切、无重叠 |

**仍未验证**：以上是「没画错、看得清、没裁切」的证据，不是「好看」的证据。
审美判断需要人眼，本次会话的模型不能读图（见 §8 第 1 条）。上述测量先在本地
debug 构建上迭代，随后用同一份工具在**安装的 CI release 产物**上重放一遍，
结论一致（§7），因此这些数字描述的就是使用者会看到的那份构建。

### 5.3 验证过程中的一次误写与恢复（如实记录）

为了驱动「面板回到上次分区」这条真实路径，验证脚本需要先写 InSync 的
`window-state.json`。第一次写错了目录：键被写进了 **BongoCat 自己的**开发数据根
（`~/Library/Application Support/com.ayangweb.bongo-cat/development/`），而 InSync
的数据根是 `com.ayangweb.insync`。发现后立即删除该键并把文件写回原内容
（`{"schema_version":1,"settings_window":null,"overlay_window":{…}}`，与原始文件
语义一致，已复核）。此后所有驱动都只写 InSync 自己的数据根。这是 InSync 首次、
也是唯一一次触碰 BongoCat 数据根，属验证脚本失误而非产品路径（产品代码从未
引用 BongoCat 的根，`data_root()` 只有 `com.ayangweb.insync`）。测试留下的面板
分区已复位为默认 `chat`。

## 6. 后端部署方法和实际部署状态

- 交付物：仓库根 `Dockerfile`（两阶段 rust 构建，非 root，健康检查）+
  `docker-compose.yml`（127.0.0.1:8787、命名卷、healthcheck）。
- 文档：`docs/insync/deployment.md`（环境变量、Caddy/nginx TLS/WSS 配置、
  sqlite 热备/恢复命令、安全清单）。
- **实际部署状态：未部署到任何公网服务器**（无授权的域名/主机凭证）。本地
  Docker 镜像未在此环境构建（无 docker daemon 权限验证），但 CI 中包含镜像
  构建与健康检查 job。

## 7. 可执行文件与交付物路径

| 交付物 | 路径 | 状态 |
| --- | --- | --- |
| 桌面应用（macOS debug 冒烟二进制） | `target/debug/insync-app` | 本地构建 ✅（本地 `target/` 已于 2026-10-11 删除，见 §8 第 9 条） |
| 桌面应用 + 服务器（macOS aarch64 release） | CI artifact `insync-macos-aarch64` | 由 CI 产出 |
| 桌面应用 + 服务器（Windows x86_64 release） | CI artifact `insync-windows-x86_64` | 由 CI 产出 |
| 服务器（本地构建） | `cargo build -p insync-server` → `target/debug/insync-server` | 本地构建 ✅（release 由 CI） |
| 精灵素材 + manifest | `resources/insync/sprites/`、`resources/insync/icons/tray.png` | 已提交 ✅ |
| 提醒音 | `resources/insync/sounds/alarm.wav` | 已提交 ✅（生成器 `tools/insync-audio/`） |
| 预览 GIF / contact-sheet | `target/insync-preview/` | 本地产物（未入库） |
| 冒烟证据 | `target/insync-preview/smoke-evidence.txt` | 本地产物 |
| CI 工作流 | `.github/workflows/insync-verify.yml` | 已提交并推送，`feat/insync` 上实际运行 ✅ |
| 发布通道（ADR-0092） | `.github/workflows/insync-release.yml`、`tools/insync-release.py`、`docs/insync/CHANGELOG.md` | 已提交 ✅ 且已实际发布：<https://github.com/SU0510/BongoCat/releases/tag/insync-v0.1.0>（四个平台资产 + `SHA256SUMS`，复核见 §7.2） |
| Windows 交叉验证 | `cargo xwin clippy --target x86_64-pc-windows-msvc`（六 crate，`-D warnings`） | 本机通过 ✅ |
| 部署配置 | `Dockerfile`、`docker-compose.yml`、`docs/insync/deployment.md` | 已提交 ✅ |

CI 产出的 release 二进制哈希（`insync-macos-aarch64`：本轮之前安装的是
`3939521b`（run 38113466225，含 §5.1 的两处修复）`insync-app` =
`8e46e665…`；此前的 `326408e1`（run 38108110093）`insync-app` =
`b58b5b1a…`。`insync-server` 两次均为 `293f0ddd…`；
`insync-windows-x86_64`（`326408e1`）：`insync-app.exe` = `fc273b9f…`、
`insync-server.exe` = `ba017f4a…`），其中 macOS 两份已**安装到本机**：

| 安装位置 | 内容 | 说明 |
| --- | --- | --- |
| `~/Applications/InSync.app` | `Contents/MacOS/insync-app` + `Info.plist` | Bundle ID `com.ayangweb.insync`，ad-hoc 签名（`codesign --force --sign -`），`codesign --verify` 通过 ✅ |
| `~/.local/bin/insync-server` | 服务器二进制 | 已 ad-hoc 签名并验证 ✅ |

安装后二进制哈希与 CI 产物不同，原因是 ad-hoc 签名会重写 Mach-O 的签名段
（文件恰好增长 18128 字节）。比对方式是 `otool -s` 逐个 section 取出内容、
**忽略地址列**：签名会新增一条 load command，使所有 section 的起始地址整体
后移，因此直接比对含地址列的原始输出会对同一份产物误报 0/22（本轮先踩到，
改正后得到下面的结果）。本轮 app 的 **22/22 个 section 内容逐字节一致**，
server 的 17/17 在上一轮同样一致；同一比对对上一轮 run 的产物给出 11 个
section 不同，说明该检查确实能发现差异。本地
安装使用 `-`（ad-hoc）证书，Gatekeeper 首次打开仍需右键“打开”；正式分发
签名的缺少见 §8。

当前安装内容为面板视觉重做这一轮的 CI 产物（`f638966c`，run 38119777698，
五个任务全绿：assets 12s / test-linux 39s / build-macos 5m55s /
build-windows 8m55s / docker-server 1m52s）：`insync-app` =
`66fc76ae…`、`insync-server` = `293f0ddd…`（与上一轮逐字节相同，服务端代码
未变，说明 macOS 构建可复现）、Windows 产物 `insync-app.exe` =
`bb6b455c…`、`insync-server.exe` = `7726dee1…`（与上一轮不同；Windows PE
跨 runner 不是逐字节可复现的，这与 macOS 侧的稳定结果并不矛盾，两者都只说明
各自平台的表现）。安装后再跑一次 `verify_panel.py --app <安装的 release 二进制>`
把 §5.2 的八分区测量重放一遍，结论与 debug 构建完全一致（选中磁贴正确、
`stray_edge=0`、配色与卡片占比相同），因此**用户看到的安装版就是被测量的那份
构建**。签名后的 22/22 section 比对同样针对这一轮产物重跑通过。

### 7.1 CI 实际运行记录（触发问题与修复）

分支推送后 `insync-verify` 一共暴露出六个真实问题，均已修复并留下提交：

1. **触发器 glob 不跨目录**：`paths: crates/insync-*` 的单星号不匹配
   `crates/insync-sprite/src/library.rs`，只匹配 crate 目录本身，导致此后
   所有纯代码改动**完全没有触发 CI**（表现为「push 后没有 run」）。改为
   `crates/insync-*/**`。
2. **Linux 任务误构建 GPUI**：`yeslogic-fontconfig-sys` 在 ubuntu runner 上
   找不到系统 fontconfig 而失败。GPUI 需要整套 Linux GUI 开发栈，而 InSync
   只在 Windows/macOS 发布；Linux 任务改为只跑共享 crate（core/server/
   client/sprite），`insync-pet-window` 与 `insync-app` 由 macOS/Windows 任务
   lint + 测试（与 BongoCat `verify.yml` 对自身 GPUI 产品的划线一致）。
3. **Windows 任务 clippy 步骤级 schema 错误**：`targets:` 写在 step 级而非
   `with:` 内，使整份 workflow 无效（0 job、run 名回退成文件名）。
4. **素材防漂移跨平台不稳定**：PNG 的 zlib 输出在 macOS 与 Ubuntu 之间不同，
   字节比对必然失败。改为解码后按像素比较（`check_pixel_drift.py`），
   并固定 `pillow==12.3.0 numpy==2.5.3`。

CI 补上 Linux/macOS/Windows 三个平台的真机测试后，又暴露出两个**只有真平台
才会出现**的产品缺陷（此前都只在本机 macOS 验证过）：

5. **Windows 上没有时区数据库**：workspace 关闭了 jiff 默认特性，只启用
   `tz-system` 与 `tzdb-zoneinfo`，两者都读操作系统的时区库；Windows 没有，
   于是 `Zoned::now()` / `TimeZone::get("UTC")` 在 Windows 上直接失败——这
   不只是测试问题，闹钟线程与时钟走的就是这条路径。已启用 jiff 的
   `tzdb-bundle-platform`（仅在无系统时区库的平台链接内置库，macOS/Linux
   继续读系统库）。Windows 目标已用 `cargo xwin clippy --target
   x86_64-pc-windows-msvc -p insync-app --all-targets -- -D warnings` 复核。
6. **容器数据目录归属错误**：运行阶段以 `USER 1000:1000` 运行且声明
   `VOLUME /data`，但镜像里没有 `/data`，Docker 创建的卷归 root，服务端以
   `open db: unable to open database file` 启动失败（CI 冒烟实测到这个错误）。
   已在镜像内 `mkdir -p /data && chown 1000:1000 /data`，并安装健康检查所需
   的 `curl`。**注意**：本机没有 Docker daemon，此修复未经本地构建验证，
   由 CI 的 `docker-server` job（构建镜像 + 启动 + 探测 `/health`）验证。

另外，`tools/tests/test_product_version_contract.py` 捕获到一个此前未被发现的
契约违反：六个 InSync crate 没有声明版本，而仓库要求所有 workspace 成员继承
`[workspace.package].version`。当时的处理是改为 `version.workspace = true` 并同步
`Cargo.lock`（2.2.0）——这解决了契约问题，但把 InSync 的版本号绑死在 BongoCat 上。
后来按 ADR-0092 重新决定：InSync 用自己的版本号（0.1.0），六个 crate 显式声明，
契约测试改为只约束 BongoCat 自己的 crate；两个决定的前后关系写在 ADR-0092 里。

修复后的最终运行：**run 38108110093（commit `326408e1`）五个 job 全绿** ——
`assets`、`test-linux`、`build-windows`、`docker-server`、`build-macos` 全部
`success`。§5.1 的两处修复之后：**run 38113466225（commit `3939521b`）同样五个
job 全绿**，本机安装的二进制即该 run 的产物，并在其上复验了透明背景、启动时面板
置前、菜单项“打开面板”（已打开时不关闭）、关闭按钮后再打开（新建窗口并置前）与
桌宠右键两条分支。后续仅有文档提交（`docs/` 不在触发路径内），因此代码状态的验证
结论仍以 `3939521b` 的这次 run 为准。

### 7.2 首次真实发布（`insync-v0.1.0`）与发布通道的验证

发布通道（ADR-0092）的代码在提交 `fec6bf8e`，发布提交是 `1ed9d3ff`
（`chore(insync): release v0.1.0`），附注标签 `insync-v0.1.0` 打在同一提交上并推送。
链路与结果：

| 环节 | 结果 |
| --- | --- |
| `insync-verify` run [38122874262](https://github.com/SU0510/BongoCat/actions/runs/38122874262)（commit `1ed9d3ff`） | 六个任务全绿：`assets`、`contracts`（19 条 InSync 契约）、`test-linux`、`build-macos`、`build-windows`、`docker-server` ✅ |
| `insync-release` run [38123464194](https://github.com/SU0510/BongoCat/actions/runs/38123464194)（tag push） | `publish` 七个步骤全部 success：标签=版本校验、提交标题校验、定位该提交的 verify 运行、`gh run download`、校验和、正文合成、`gh release create` ✅ |
| Release | https://github.com/SU0510/BongoCat/releases/tag/insync-v0.1.0（标题 `InSync 0.1.0`，非 draft、非 prerelease） |
| 资产 | `insync-app-macos-aarch64` 16,802,544 B、`insync-app-windows-x86_64.exe` 20,483,072 B、`insync-server-macos-aarch64` 3,210,144 B、`insync-server-windows-x86_64.exe` 3,240,960 B、`SHA256SUMS` ✅ |

自己动手复验（不只看 CI 的结论）：

1. `gh release download insync-v0.1.0` 取回五个文件后执行 `shasum -a 256 -c SHA256SUMS`：
   四个资产全部 `OK`。
2. 再 `gh run download 38122874262` 取回被验证过的 CI 产物，逐个比对 SHA-256：发布出去的
   四个文件与那次运行产出的四个文件**哈希完全相同**（app/服务器 × macOS/Windows），
   即「发布的字节 = 被验证的字节」不是工作流自述，而是可复核的事实。
3. 反向验证（负向对照）：在同一提交上推一个版本号故意不匹配的临时标签 `insync-v0.2.0`，
   run [38123689217](https://github.com/SU0510/BongoCat/actions/runs/38123689217) 在第一步
   即失败，日志为 `tag 'insync-v0.2.0' does not match the InSync version: expected
   'insync-v0.1.0'`，且**没有创建任何 release**（`gh release list` 只有 `insync-v0.1.0`）；
   随后该临时标签已在本地与远端删除，仓库里只留 `insync-v0.1.0`。

发布正文由上表第 2 步从 `docs/insync/CHANGELOG.md` 的 `## 0.1.0 - 2026-10-11` 条目生成，
并加了指向该 verify 运行的链接与「裸二进制、未签名、平台限制」说明。

发布通道本身还暴露出一个只有真跑才会发现的问题并已修掉：`insync-release.yml` 原本声明了
`workflow_dispatch` 以便手动重发，但 GitHub 只为**默认分支上的**工作流文件注册该触发器，
本文件在功能分支上，`gh workflow run` 直接返回 `HTTP 422: Workflow does not have
'workflow_dispatch' trigger`——即那是一段永远不会生效的配置。现已删除该触发器，重发路径
改为重跑失败的那次 run（同一标签、同一提交、同样的校验），并补了契约测试防止它被无声加回。
（已发布的 `insync-v0.1.0` 由 `1ed9d3ff` 上的旧版文件产出，那次运行未用到该触发器。）

## 8. 未完成与待办（如实标注）

1. **观感目检仍需使用者判断**：显示器可用后，窗口级行为已能在实机测量并据此
   修复了三个缺陷（§5.1：z-order、点击路径、逐像素透明性；§5.2：面板视觉）。
   但「桌宠看起来对不对、面板排版好不好看」属于审美判断：本次会话的模型不能读取
   图像，无法自己看截图，因此这类判断仍以使用者的反馈为准（本次三个缺陷正是这样
   发现的）。Windows 侧同理需要实机目检。
2. **Windows 实机运行未验证**：Windows 目标的可构建性已通过 `cargo xwin`
   交叉编译检查（六 crate 全绿，含全部 Windows 依赖路径），并在 CI 的
   Windows runner 上跑通了 lint 与全部单元/集成测试（由此发现并修复了
   Windows 缺少时区数据库的缺陷）；剩余缺口仅是**在真实 Win32 桌面会话中
   运行确认**（分层窗口行为、DPI、托盘）。
3. **OS 原生通知**：当前通知为应用内通知中心 + 声音 + 桌宠动画；未接
   UNUserNotificationCenter / Windows Toast（涉及打包与权限，v2 工作）。
4. **安装包流水线**：InSync 仍**未**接入 `bongocat-packaging`（app/dmg/NSIS），
   也没有签名、公证与自动更新——发布通道（ADR-0092）发的是裸二进制 +
   校验和 + changelog 正文，正文里如实写明未签名与平台限制。安装器、签名与
   自动更新仍是 v2 工作。
5. **公网端到端**：无授权的服务器/域名，未做跨公网 WSS 的真机联调（本地
   TCP 集成测试覆盖同一路径，但 TLS/代理行为未实测）。
6. **素材精修**：像素画为程序化生成，质量达标但可继续精修（尤其 walk 与
   poke_react 的中间帧）；生成-校验-预览管线已就绪。
7. **hub 面板 UI 细节**：列表为简单行式布局，未做滚动虚拟化与复杂气泡
   样式；便签编辑为「选中→编辑→保存」而非即时自动保存。（视觉重做后，颜色、
   字号层级与控件样式均已由 `palette` + gpui-kit 组件承载，见 §5.2。）
8. **`insync-core` 中 `EventData` 对 `letter.read` 携带 `LetterId`**，若未来
   需要跨设备同步留言已读人的列表，需要扩展（当前单收件人模型够用）。
9. **验证策略改为「只在 CI 跑」（2026-10-11，使用者要求）**：本机不再执行
   `cargo fmt/clippy/test/build` 与交叉编译验证，原因是一次工作区级验证把
   `target/` 涨到 17G 并让 228GB 磁盘写满（Bash 工具连日志文件都写不出来），
   只能删掉 `target/` 恢复。此后每次验证 = 推送分支 + 读 `insync-verify` 结果。
   两点后果如实说明：其一，未推送的改动在本机没有任何自动化验证；其二，
   **只改文档的提交不会触发 `insync-verify`**（它的 `paths` 不含 `docs/`，唯一例外是
   `docs/insync/CHANGELOG.md`——发布正文取自它、分段标题也由契约测试校验，所以它算
   构建输入），这类提交由「父提交的 CI 已全绿 + 改动不含任何构建输入」共同保证，
   而不是重新跑一遍 CI。

## 9. 需要用户亲自完成的最少事项

1. **在真实设备上目检**：分支已推送、CI 已跑通，macOS 产物已装入本机
   `~/Applications/InSync.app`（本轮的安装内容与哈希见 §7），托盘“打开面板”、
   桌宠右键与透明背景已在实机复验（§5.1），面板视觉的像素测量也已在安装版上
   重放（§5.2、§7）。仍需**人眼确认观感**（新配色和排版是否讨喜、桌宠动作是否
   自然——本次会话的模型读不了图像）；Windows 侧请从 artifact
   `insync-windows-x86_64` 下载后在实机运行。
2. **提供一台公网服务器 + 域名**（可选）：按 `docs/insync/deployment.md`
   执行 `docker compose up -d --build` + 反向代理 TLS；把地址填进两端
   客户端设置。
3. **决定 InSync 的发布渠道要不要再往前一步**：现在已有独立版本号与发布通道
   （ADR-0092：`insync-v*` 标签 → 裸二进制 + 校验和 + changelog 正文），
   仍未决定的是要不要接入 bongocat-packaging 生成安装包、签名/公证与自动更新。
4. （如需合入 master）基于 `feat/insync` 创建 PR。
