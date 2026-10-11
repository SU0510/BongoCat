# InSync 最终交付报告

日期：2026-10-11　分支：`feat/insync`（自 `catdog` = `origin/master@65425b75` 切出）

## 1. 实际完成的产品功能

**桌宠（A 组验收）— 已完成**

- 原创像素人物「Momo」（程序化生成、双配色变体 momo / momo-mint，48×64 逻辑分辨率 4x 输出）。
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

- 79 个 InSync 自动化测试全部通过（见 §4）。
- macOS aarch64 实机构建 + 冒烟通过；Windows x86_64 交叉 clippy 检查通过，构建交给 CI。
- 已推送到 `origin/feat/insync`，`.github/workflows/insync-verify.yml` 实际运行并逐项修好触发与构建问题（见 §7）。
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
决策见 `docs/adr/0089/0090/0091`。

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
| insync-app（store 5 + clock 7 + notify 3 + 铃声解码 1） | 14 | ✅（不含 GUI 自动化） |
| bongocat-audio（回归：启用 wav feature 后原有测试） | 15（+1 ignored） | ✅ |
| 素材校验 `validate_sprites.py` | 26 sheet | ✅ |
| 素材校验 `validate_alarm.py`（格式/电平/削波/音高） | 1 WAV | ✅ |
| BongoCat 工具契约测试 `tools/tests/` | 102 | ✅（含 workspace 版本契约与 workflow 校验） |

命令：`cargo test --locked -p insync-core -p insync-server -p insync-client -p insync-sprite
-p insync-pet-window -p insync-app -p bongocat-audio`（79 + 15 通过）。
另有 `cargo clippy --locked <六个 insync crate> -p bongocat-audio --all-targets -- -D warnings`
全绿（pet-window 另在 `x86_64-pc-windows-msvc` target 上全绿），以及
`cargo fmt --all -- --check` 与 `python3 -m unittest discover -s tools/tests` 102 项全绿。

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
- **人工试用（使用者本人，安装版）**：试用暴露了两个自动化验证覆盖不到的
  缺陷，均已定位、修复并复验（详见 §5.1）。

### 5.1 人工试用暴露的两个缺陷（已修复）

| 现象 | 根因 | 修复与验证 |
| --- | --- | --- |
| 托盘“打开面板”点击无反应 | 见下方两条 | `hub::show_hub` / `toggle_hub` 拆分 |
| 桌宠背后是一块实色矩形 | NSPanel 只设了 `setOpaque(false)`，未清 `backgroundColor` | macOS 后端补 `setBackgroundColor(clearColor)` |

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
| 桌面应用（macOS debug 冒烟二进制） | `target/debug/insync-app` | 本地构建 ✅ |
| 桌面应用 + 服务器（macOS aarch64 release） | CI artifact `insync-macos-aarch64` | 由 CI 产出 |
| 桌面应用 + 服务器（Windows x86_64 release） | CI artifact `insync-windows-x86_64` | 由 CI 产出 |
| 服务器（本地构建） | `cargo build -p insync-server` → `target/debug/insync-server` | 本地构建 ✅（release 由 CI） |
| 精灵素材 + manifest | `resources/insync/sprites/`、`resources/insync/icons/tray.png` | 已提交 ✅ |
| 提醒音 | `resources/insync/sounds/alarm.wav` | 已提交 ✅（生成器 `tools/insync-audio/`） |
| 预览 GIF / contact-sheet | `target/insync-preview/` | 本地产物（未入库） |
| 冒烟证据 | `target/insync-preview/smoke-evidence.txt` | 本地产物 |
| CI 工作流 | `.github/workflows/insync-verify.yml` | 已提交并推送，`feat/insync` 上实际运行 ✅ |
| Windows 交叉验证 | `cargo xwin clippy --target x86_64-pc-windows-msvc`（六 crate，`-D warnings`） | 本机通过 ✅ |
| 部署配置 | `Dockerfile`、`docker-compose.yml`、`docs/insync/deployment.md` | 已提交 ✅ |

CI 产出的 release 二进制哈希（`insync-macos-aarch64`：当前安装的来自
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
（两个文件都恰好增长 18128 字节）。已用
`otool -s` 逐 section 比对证明安装内容是 CI 产物本身：**app 的 22/22、
server 的 17/17 个 section 内容逐字节一致**，差异仅在签名元数据；同一比对
对上一轮 run 的产物给出 11 个 section 不同，说明该检查确实能发现差异。本地
安装使用 `-`（ad-hoc）证书，Gatekeeper 首次打开仍需右键“打开”；正式分发
签名的缺少见 §8。

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
契约违反：六个 InSync crate 使用硬编码 `version = "0.1.0"`，而仓库要求所有
workspace 成员继承 `[workspace.package].version`。已改为
`version.workspace = true` 并同步 `Cargo.lock`（改为 2.2.0）。

修复后的最终运行：**run 38108110093（commit `326408e1`）五个 job 全绿** ——
`assets`、`test-linux`、`build-windows`、`docker-server`、`build-macos` 全部
`success`。§5.1 的两处修复之后：**run 38113466225（commit `3939521b`）同样五个
job 全绿**，本机安装的二进制即该 run 的产物，并在其上复验了透明背景、启动时面板
置前、菜单项“打开面板”（已打开时不关闭）、关闭按钮后再打开（新建窗口并置前）与
桌宠右键两条分支。后续仅有文档提交（`docs/` 不在触发路径内），因此代码状态的验证
结论仍以 `3939521b` 的这次 run 为准。

## 8. 未完成与待办（如实标注）

1. **观感目检仍需使用者判断**：显示器可用后，窗口级行为已能在实机测量并据此
   修复了两个缺陷（§5.1：z-order、点击路径、逐像素透明性）。但「桌宠看起来对不对、
   面板排版好不好看」属于审美判断：本次会话的模型不能读取图像，无法自己看截图，
   因此这类判断仍以使用者的反馈为准（本次两个缺陷正是这样发现的）。Windows 侧
   同理需要实机目检。
2. **Windows 实机运行未验证**：Windows 目标的可构建性已通过 `cargo xwin`
   交叉编译检查（六 crate 全绿，含全部 Windows 依赖路径），并在 CI 的
   Windows runner 上跑通了 lint 与全部单元/集成测试（由此发现并修复了
   Windows 缺少时区数据库的缺陷）；剩余缺口仅是**在真实 Win32 桌面会话中
   运行确认**（分层窗口行为、DPI、托盘）。
3. **OS 原生通知**：当前通知为应用内通知中心 + 声音 + 桌宠动画；未接
   UNUserNotificationCenter / Windows Toast（涉及打包与权限，v2 工作）。
4. **安装包流水线**：InSync 未接入 `bongocat-packaging`（app/dmg/NSIS）；
   CI 产出裸二进制。安装器、签名、自动更新均未实现。
5. **公网端到端**：无授权的服务器/域名，未做跨公网 WSS 的真机联调（本地
   TCP 集成测试覆盖同一路径，但 TLS/代理行为未实测）。
6. **素材精修**：像素画为程序化生成，质量达标但可继续精修（尤其 walk 与
   poke_react 的中间帧）；生成-校验-预览管线已就绪。
7. **hub 面板 UI 细节**：列表为简单行式布局，未做滚动虚拟化与复杂气泡
   样式；便签编辑为「选中→编辑→保存」而非即时自动保存。
8. **`insync-core` 中 `EventData` 对 `letter.read` 携带 `LetterId`**，若未来
   需要跨设备同步留言已读人的列表，需要扩展（当前单收件人模型够用）。

## 9. 需要用户亲自完成的最少事项

1. **在真实设备上目检**：分支已推送、CI 已跑通，macOS 产物已装入本机
   `~/Applications/InSync.app`（见 §7），托盘“打开面板”、桌宠右键与透明背景
   已在实机复验（§5.1）。仍需**人眼确认观感**（桌宠动作是否自然、面板排版是否
   合理——本次会话的模型读不了图像）；Windows 侧请从 artifact
   `insync-windows-x86_64` 下载后在实机运行。
2. **提供一台公网服务器 + 域名**（可选）：按 `docs/insync/deployment.md`
   执行 `docker compose up -d --build` + 反向代理 TLS；把地址填进两端
   客户端设置。
3. **决定 InSync 的发布渠道**：是否接入 bongocat-packaging 生成安装包/
   自动更新，或保持 CI 裸二进制分发。
4. （如需合入 master）基于 `feat/insync` 创建 PR。
