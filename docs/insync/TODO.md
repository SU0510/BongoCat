# InSync TODO

每个任务先有自动化验证，再标记完成。完成后在括号内附验证命令与结果摘要。

## Phase A — 设计与素材

- [x] A1 源码审计 + 架构决策（ADR-0089/0090/0091、docs/insync/technical-design.md、protocol.md、sprite-assets.md）
- [x] A2 素材生成管线：`tools/insync-sprites/{generate,validate,preview}_sprites.py`，13 个动画全绿（`validate_sprites.py` OK；CI 任务 `assets` 防漂移）；提醒音 `tools/insync-audio/{generate,validate}_alarm.py`（纯 stdlib 确定性合成，`generate_alarm.py --check` 字节比对 + `validate_alarm.py` 校验格式/电平/音高，由同一 CI 任务执行）
- [x] A3 `insync-sprite`：manifest 加载、帧计算、优先级状态机、随机行为（13 个单元测试通过）
- [x] A4 新角色「Nana」（按使用者真实照片 redesign：深棕色双麻花辫搭在肩膀身前、
  中分窗帘刘海（发丝自中分沿发际线向两侧梳下）、玫瑰粉无袖上衣与斜挎包带）：生成器加 `Style` 结构旗标
  （`braids` / `center_part` / `sleeveless` / `strap` / `soft_face` / `ahoge`），绘制函数按旗标
  分支而非角色名分支——momo/mint 输出经 4x 最近邻像素比对确认零漂移。第一版
  观感发黄发黑、辫子垂在身后两侧，按使用者反馈返工：肤色提亮为暖 ivory + 软暖褐
  描边，`soft_face` 瓜子脸（眼睛一行保持整脸宽度、两段轻收到圆润小下巴、下巴
  不留硬角），辫子贴脸而下、搭在肩膀身前垂到发绳（渲染顺序：躯干后、手臂/包带前，
  举臂不穿模，包盖发梢）；辫身每两行外鼓一像素读出麻花阶梯感。
  **2026-10-12 回退**：此后又做了两轮改动——Q 版大头短腿比例（`big_head`，头
  y5..31、躯干 y32..45、短腿厚鞋，落地线仍在 y59，避免切角色时悬空）与「顺发际线
  向后梳、无刘海」发型（`combed_back`）——使用者看过安装版后判定**越改越不对**
  （「完全不还原」「额头很大」），要求回到两轮之前，于是这两组旗标与其素材整体
  撤掉：`tools/insync-sprites/generate_sprites.py`、`tools/insync-ui/generate_ui_images.py`、
  nana 的 13 张精灵图与 2 张裁切图都从 `a76a7a4c` 原样取回
  （`git diff a76a7a4c -- <这些路径> resources/insync` 为空）。验证：`validate_sprites.py`
  对 manifest 三个角色全绿；`check_pixel_drift.py`（重新生成到临时目录后逐像素
  比对）报告无漂移；`generate_ui_images.py --check` 四张裁切图全部与重新生成一致；
  momo / momo-mint 的 26 张精灵图自始至终未被触及。
  **2026-10-12 窗帘刘海轮**（收尾上述遗留）：在 v1 脸型之上**只改发型一处**——空气刘海
  换窗帘刘海：发丝自中分沿发际线斜向太阳穴梳下，额头读作向眉线加宽的窄三角，更贴近
  原型照片；脸型、比例、辫子与衣着全部未动。重新生成后 `validate_sprites.py` 全绿、
  `check_pixel_drift.py` 无漂移、momo / momo-mint 零漂移。
  **2026-10-12 Q 版轮**：见 A5——Q 版做成**独立角色** `nana-chibi`，nana 本人不再动比例。
  这两轮的审美已由使用者人眼确认（窗帘刘海轮：「现在正常版非常好」；Q 版轮见 A5），
  nana 的素材就此定稿。
- [x] A5 新角色「Nana（Q 版）」（学 Q 版二次元参考图的**画法**，画的仍是照片里的 nana）：
  大圆眼（上排 2×2 高光 + 下眼睑一颗）、腮红、辫尖五瓣小花；双辫、窗帘刘海、玫瑰粉
  无袖上衣、斜挎包带与牛仔裤全部保留。生成器新增 `chibi` 结构旗标与独立绘制
  路径（`draw_chibi_*`，自有坐标布局，不复用 nana 的体型分支），nana 的输出不受任何影响；
  调色板与 nana 逐色相同（仅 `confetti_a` 取上衣色），两者一眼读作同一个人；13 个动画与
  Momo 同套。`generate_ui_images.py` 改为按角色裁切窗口（`PORTRAIT_CROPS`）：Q 版头部裁切
  高大于宽，裁后透明补方再缩放，方形头像框不拉伸；wave 图按角色取尺寸。app 侧注册：
  `store::CHARACTERS` 加 id、`assets.rs` 内嵌 13 张表与 2 张裁切图、`i18n.rs` 加
  `character.nana-chibi`；设置面板角色行按 `store::CHARACTERS` 循环渲染，第三个选项自动
  出现，旧配置无该值仍回退首个角色。
  **2026-10-13 使用者逐轮反馈定稿**（每轮只改一处，改完重出 contact sheet 交人看）：
  ① 张口笑 → **不张口的 v 形笑嘴**（y32..33）；② 编麻花 + 搭肩垂胸 → **垂直发束**
  （不搭肩，自头部侧发列直垂到 y48，坐姿收到 y44 使发梢不落地）；③「整体太瘦长」→
  头顶压平四行（头 y6..38、x9..38），脸只四段收到圆润下巴，头部读作圆；④「有些图
  错位」→ 辫子与头同取 `lean` 偏移，举臂移到头部轮廓**外侧**（x5..7 / x40..42）并用
  y41..43 的袖子横向接回躯干，渲染顺序改为 legs → torso → arms → braids → strap →
  head（袖子先画、辫子后画，袖子从发束后面穿过，既不消失在头后也不与躯干断开）。
  使用者看过第 4 轮预览后要求把这一版做进软件、提交推送打标签，即接受该版。
  验证：`validate_sprites.py` 对 manifest 四个角色全绿；`check_pixel_drift.py` 重新生成后
  报告无漂移；`generate_ui_images.py --check` 六张裁切图全部一致（`nana-chibi` 头像
  96×96、墨迹占 78%；wave 133×194、墨迹占 67%）；momo / momo-mint 零漂移；
  `every_character_is_fully_embedded` / `every_panel_image_is_embedded` 覆盖新角色
  （Rust 测试按约定交 CI 跑，本地不执行）。

## Phase B — 桌宠窗口

- [x] B1 `insync-pet-window` macOS NSPanel 后端（透明/置顶/拖动/点击/右键/隐藏）
- [x] B2 `insync-pet-window` Windows 分层窗口后端（UpdateLayeredWindow、HTCAPTION 拖动、右键、DPI；`cargo clippy --target x86_64-pc-windows-msvc` 全绿）
- [x] B3 位置持久化、缩放（50–400%）、恢复

## Phase C — 协议与服务端

- [x] C1 `insync-core`：领域模型、协议 DTO、事件信封、outbox/幂等语义（28 个单元测试）
- [x] C2 `insync-server`：auth（argon2id+token 轮换/撤销）、配对/解除、消息/留言/戳戳（限频）
- [x] C3 `insync-server`：专注状态机（revision 幂等）、共享待办/便签/纪念日、presence、事件日志（13 个集成测试含安全矩阵）
- [x] C4 集成测试：双用户绑定/消息/留言/戳戳/专注全流程/离线恢复/重复幂等/解绑撤销访问（全部通过）

## Phase D — 客户端同步

- [x] D1 `insync-client`：REST+WS、重连退避、补拉去重、presence（独立 tokio 线程；4 个引擎级集成测试）
- [x] D2 安全测试：未登录/非成员/越权 ID/token 过期撤销/解绑后访问/日志隐私（`security_matrix` 等通过）

## Phase E — 应用集成

- [x] E1 `insync-app` 骨架：托盘、桌宠窗口、点击/戳戳反应、窗口状态持久化
- [x] E2 本地 store：settings/account/window-state/personal/sync 全套 JSON store + 损坏恢复（5 个测试）
- [x] E3 聊天窗口 + 留言 + 戳戳面板 + 在线状态（hub 8 标签页）
- [x] E4 共同专注面板（邀请/接受/倒计时/暂停/提前结束/勿扰）
- [x] E5 共享待办/便签/纪念日面板 + 冲突提示（便签 revision 冲突 → 通知，不静默覆盖）
- [x] E6 闹钟（jiff、DST、错过触发恢复、声音）+ 通知中心（节流/勿扰/历史）；提醒音为内嵌生成的 WAV，启动时 `Prepare` 一次（`bongocat-audio` 只播放已解码片段，否则 `Play` 静默失败），`insync-app` 有解码回归测试
- [x] E7 设置面板（注册/登录/邀请码/绑定/解绑/登出/语言切换）+ 断线/过期状态提示
- [x] E9 面板视觉重做（可爱化）：`insync-app::palette` 把桌宠精灵的奶油/暖褐/
  玫瑰/蜜桃取色写成 gpui-kit `Theme`（浅色 cream + 深色 cocoa 两套独立调校，
  含 27 条 WCAG 对比度断言），页头（吉祥物头像 + 配对状态）、8 个带图标的 tab
  磁贴、「卡片 + 气泡 + 空态插图」的内容骨架全部改为读主题令牌。
  迁移组件（依据 gpui-kit 官方文档 [components](https://gpui-kit.com/docs/components)
  与 [docs.rs/gpui-kit](https://docs.rs/gpui-kit)，未凭记忆猜 API）：
  `Button`（+`ButtonVariants`，默认/outline/ghost 三级）、`Avatar`、`Bubble`
  （`BubbleVariant::Filled`）、`Checkbox`、`Switch`、`Input`/`InputState`、
  `ProgressCircle`、`Empty`（`EmptyMedia`/`EmptyTitle`/`EmptyDescription`）、
  `Icon`+`IconName`、`Root`、`Size`/`Sizable`、`ActiveTheme`。剩余特殊控件：无
  （面板内没有自定义绘制控件）；自有的只剩布局组合 `card()`/`section()`。
  图标逐个核对存在的 `IconName` 变体（解析生成的 `icon_name.rs`，18/18 命中）。
  验证：`verify_panel.py --all` 对 8 个分区各截一次真实窗口（驱动真实「回到
  上次分区」路径），断言选中磁贴正确、`stray_edge=0`（无裁切）；另测 400pt 与
  最小 360pt 宽、浅/深两套调色板（正文对比度 6.9–8.5:1）。`insync-app` 23 个
  测试通过、`cargo clippy -D warnings` 干净。
- [x] E11 角色选择：`Settings.character`（`#[serde(default)]` 默认 `momo`，
  validate 未知值回退首个角色，旧文件无该字段照常读取）→ `app::CURRENT_CHARACTER`
  → tick 时 `AnimationPlayer::set_sprites` 热切换（played_character 记账，每档只切
  一次）；面板头像/空态插图按当前角色取 `insync/{character}_*.png`；设置面板偏好
  卡新增角色行（两个按钮，选中态用 `Selectable`），文案双语（`character.momo` /
  `character.nana`）。验证：`insync-app` 27 个测试通过（新增旧数据回归
  `settings_without_a_character_still_load`、未知角色回退
  `an_unknown_character_falls_back_to_the_first`、三角色全内嵌
  `every_character_is_fully_embedded`、面板图内嵌 `every_panel_image_is_embedded`）；
  实机切换动画与头像待使用者确认。

## Phase F — 交付

- [x] F1 Dockerfile + docker-compose.yml + deployment.md（备份/恢复/TLS/健康检查）
- [x] F2 CI workflow `.github/workflows/insync-verify.yml`（`assets` 素材校验与防漂移 → Linux 共享 crate 测试 → macOS 全 crate clippy/测试 + release 构建 → Windows 全 crate clippy/测试 + release 构建 → Docker 镜像冒烟）。已推送并实际跑通诊断：修复触发器 `crates/insync-*` 单星号不跨 `/` 导致代码改动不触发、Linux 任务误构建 GPUI 导致 fontconfig 缺失、素材漂移字节比对跨平台不稳定、六个 crate 未继承 workspace 版本（违反 `test_product_version_contract`）。三个平台真机测试后另发现并修复两个产品缺陷：Windows 无系统时区库导致时钟/闹钟失败（jiff `tzdb-bundle-platform`）、容器 `/data` 归 root 导致服务端无法建库（镜像内预建并 chown，另装 `curl` 供健康检查）。
- [x] F3 本地 macOS 构建 + 冒烟（进程稳定 35s、干净退出、RSS 平稳 ~118MB、CPU ~6–8% debug 构建；当时全部 79 个测试通过（现为 88，见最终报告 §4）；截图因测试显示器输出全黑无法目检，已如实记录。Windows 目标：六 crate `cargo xwin clippy --target x86_64-pc-windows-msvc -D warnings` 全绿，含 gpui-kit/tray-icon/muda/ring 路径）
- [x] F4 最终交付报告（docs/insync/final-report.md）
- [x] F5 独立版本号与发布通道（ADR-0092）：六个 crate 显式声明自己的版本（0.1.0，不再继承
  workspace 版本），`insync-v*` 标签触发 `insync-release.yml`，发布内容固定为「该提交上
  `insync-verify` 已通过的产物」——工作流校验标签=版本、提交标题=`chore(insync): release
  v<version>`、存在 `conclusion == "success"` 的 verify 运行，然后 `gh run download` 那次
  运行的产物（工作流里没有任何 `cargo build`），重命名为四个平台资产、生成 `SHA256SUMS`、
  正文取 `docs/insync/CHANGELOG.md` 中该版本的条目，`gh release create --verify-tag
  --notes-file` 附带全部资产。版本与发布说明的唯一读取入口 `tools/insync-release.py` 由
  `tools/tests/test_insync_release.py`（20 条，含「crate 版本不一致 / 仍继承 workspace 版本 /
  缺条目 / 条目为空 / 自造分段标题 / 无日期条目 / 不该存在的触发器」的负向对照）与
  `insync-verify.yml` 的 `contracts` 任务在每次推送时校验；`test_product_version_contract.py`
  改为只约束 BongoCat 自己的 crate。
  验证（详见最终报告 §7.2）：`insync-verify` run 38122874262（commit `1ed9d3ff`）六个任务
  全绿 → 该提交上打附注标签 `insync-v0.1.0` 并推送 → `insync-release` run 38123464194
  七步全绿，发布 [insync-v0.1.0](https://github.com/SU0510/BongoCat/releases/tag/insync-v0.1.0)
  （四个平台资产 + `SHA256SUMS` + changelog 正文）。复核：下载 Release 资产后
  `shasum -a 256 -c SHA256SUMS` 四个 `OK`，且与 verify run 的产物逐个哈希相同；负向对照
  （版本不符的临时标签 `insync-v0.2.0`）在第一步即被拒绝、未创建任何 release，临时标签已删除。
  仍未接入 `bongocat-packaging`：发布的是裸二进制，无安装包、无签名/公证、无自动更新（§8.4）。

## 已知剩余工作

见 `docs/insync/final-report.md`「未完成与待办」一节（OS 原生通知、安装包流水线、
闲时素材精修、盘怪触发的端到端公网验证等）。

## 真实使用中发现并修复的缺陷（安装版人工试用）

自动化测试与冒烟都覆盖不到「窗口是否真的在用户眼前」「窗口背景是否透明」和
「界面好不好看」，这三条由使用者试用安装版后发现：

- [x] B4 桌宠背景不透明：macOS 后端只调了 `setOpaque(false)`，AppKit 仍会用
  `backgroundColor`（默认不透明的 `windowBackgroundColor`）填满窗口，桌宠因此站在
  一块实色矩形上。按 `bongocat-overlay` 已验证的配方补上
  `setBackgroundColor(clearColor)` 与 `setAnimationBehavior(None)`。验证：截图比对
  窗口内 16 行像素与窗口外同一背景完全一致（修复前 0/16，窗口内为纯灰
  `231,231,231`）。
- [x] E8 托盘“打开面板”无反应：两个原因叠加。其一，面板窗口创建后没有任何提升
  动作，而 InSync 是 accessory（无 Dock 图标）应用、自身从不激活，未激活应用的窗口
  被排在所有其它应用窗口之后，只在浮动的桌宠之下可见；其二，该菜单项实际执行的是
  toggle，而应用启动时已经创建过面板，于是第一次点击其实**关闭**了那个看不见的
  窗口。现在 `show_hub`（启动 + 菜单项）总是打开并提升窗口、激活应用；桌宠右键为
  「已在最前则关闭，否则提升」；用户用关闭按钮关掉窗口后残留的死句柄会被丢弃并重建
  窗口。验证：窗口服务器 z-order（面板 index 22 在 Zen 的 23 之前）、真实点击菜单项
  （已打开时不再关闭；关闭后再次点击会新建窗口并置前）、真实右键（两条分支）、回归
  测试 `hub::tests::*`（3 个）。
- [x] E10 面板不好看（“面板有点丑”）：面板此前把奶油色硬编码在视图里，而组件
  仍用 gpui-kit 默认的中性灰主题，于是暖色底上叠着灰色控件，主按钮的填充又被
  调色板设成了白色而“消失”。修复分两层：产品调色板接管 `Theme`（见 E9），并按
  库的 variant 语义把默认 `Button` 的填充改为玫瑰色（`Theme::update` 的
  `reconcile` 会把 `colors` 的改动同步进组件令牌表，一处改动同时覆盖 `colors`
  与 `tokens`）。同时补两条此前只有真实窗口才暴露的契约：面板窗口订阅自身
  appearance 以跟随系统深浅色；面板停留分区写入 `window-state.json`
  （`hub_tab`，`Option` + `serde(default)`，旧文件照常读取，见技术设计
  「本地存储根」）。验证：`tools/insync-ui/verify_panel.py` 的像素测量（页头为
  muted 底 + 吉祥物蜜桃/玫瑰色块、4×2 磁贴网格中选中磁贴 x=203..292 为玫瑰、
  空态插图为静态裁切帧、主按钮 33.6% 玫瑰占比）、8 个分区截图的
  `stray_edge=0`、最小 360×500pt 下磁贴文字仍在列内（最宽 120..152 落在
  98..177）、`insync-app` 23 个测试通过（含旧 `window-state.json` 读取回归、
  面板文案 i18n 键齐全、分区名往返）与 `clippy -D warnings` 干净。
  **仍未验证**：审美本身需要人眼判断，本环境没有视觉可用的模型，只能给出上述
  可测量证据。
- [x] B5 点击区域与角色不重合（macOS）：面板此前只按「整个窗口矩形」收下鼠标，
  于是点在人物周围的透明处也会触发桌宠——桌宠的点击、右键与拖动都从那一片透
  明像素上生效。AppKit 没有 Windows 分层窗口那种按 alpha 的命中测试，只有全有
  全无的 `setIgnoresMouseEvents`，因此分两层：面板级策略按**绘制像素的外接盒**
  逐帧刷新（这样瞄准人物的点击不会因为帧采样延迟被让给后面的窗口），窗口内容
  视图再按**逐像素 alpha** 判定每次按下，透明处的按下不记录、不反应，右键与拖动
  同样只能从人物身上开始。拖动仍是 AppKit 自己的背景拖动，只是每次按下按「是否
  落在人物上」开关 `setMovableByWindowBackground`，所以没有自绘拖动循环，也不
  存在手势中途被策略打断的问题。Windows 侧分层窗口本来就按 alpha 命中，未改
  代码、只补文档。顺带修掉一个越界换算缺陷：光标坐标超出窗口时 `as usize` 会
  饱和到第 0 行，使「光标在窗口上方」可能被误判成「在人物上」，现在越界直接判为
  不在人物上。验证：`insync-pet-window` 新增 6 个 `HitTest` 单元测试（外接盒与
  alpha 读取、逐像素命中、Retina/1x 像素映射、越界不回绕、空帧让出鼠标），由 CI
  的 `build-macos` 任务执行 `cargo test -p insync-pet-window`；同一套算术另用
  Python 逐条复算 26 个断言，期望值全部一致。**仍未验证**：真实桌面上「点人物
  旁边会穿透到后面的窗口、点人物才反应」需使用者本机确认——本环境没有屏幕录制
  授权，也无法在会话里人工点击桌面。
