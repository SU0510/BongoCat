# InSync TODO

每个任务先有自动化验证，再标记完成。完成后在括号内附验证命令与结果摘要。

## Phase A — 设计与素材

- [x] A1 源码审计 + 架构决策（ADR-0089/0090/0091、docs/insync/technical-design.md、protocol.md、sprite-assets.md）
- [x] A2 素材生成管线：`tools/insync-sprites/{generate,validate,preview}_sprites.py`，13 个动画全绿（`validate_sprites.py` OK；CI 任务 `assets` 防漂移）；提醒音 `tools/insync-audio/{generate,validate}_alarm.py`（纯 stdlib 确定性合成，`generate_alarm.py --check` 字节比对 + `validate_alarm.py` 校验格式/电平/音高，由同一 CI 任务执行）
- [x] A3 `insync-sprite`：manifest 加载、帧计算、优先级状态机、随机行为（13 个单元测试通过）
- [x] A4 新角色「Nana」（按使用者真实照片 redesign，发型优先：深棕双麻花辫 + 中分
  空气刘海，保留玫瑰粉无袖上衣与斜挎包带特征）：生成器加 `Style` 结构旗标
  （`braids` / `center_part` / `sleeveless` / `strap` / `ahoge`），绘制函数按旗标
  分支而非角色名分支——momo/mint 输出经 4x 最近邻像素比对确认零漂移。辫子画在
  皮肤后层（裸臂覆盖、不穿模）、行走循环 ±1px 摆动、发绳用 accent 色。验证：
  `validate_sprites.py` 对 manifest 三个角色全绿；面板裁切图
  `{momo,nana}_{portrait,wave}.png` 由 `generate_ui_images.py --check` 覆盖漂移。
  审美本身仍待使用者人眼确认（final-report §8.1）。

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
