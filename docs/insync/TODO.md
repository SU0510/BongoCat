# InSync TODO

每个任务先有自动化验证，再标记完成。完成后在括号内附验证命令与结果摘要。

## Phase A — 设计与素材

- [x] A1 源码审计 + 架构决策（ADR-0089/0090/0091、docs/insync/technical-design.md、protocol.md、sprite-assets.md）
- [x] A2 素材生成管线：`tools/insync-sprites/{generate,validate,preview}_sprites.py`，13 个动画全绿（`validate_sprites.py` OK；CI 任务 `assets` 防漂移）；提醒音 `tools/insync-audio/{generate,validate}_alarm.py`（纯 stdlib 确定性合成，`generate_alarm.py --check` 字节比对 + `validate_alarm.py` 校验格式/电平/音高，由同一 CI 任务执行）
- [x] A3 `insync-sprite`：manifest 加载、帧计算、优先级状态机、随机行为（13 个单元测试通过）

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

## Phase F — 交付

- [x] F1 Dockerfile + docker-compose.yml + deployment.md（备份/恢复/TLS/健康检查）
- [x] F2 CI workflow `.github/workflows/insync-verify.yml`（`assets` 素材校验与防漂移 → Linux 共享 crate 测试 → macOS 全 crate clippy/测试 + release 构建 → Windows 全 crate clippy/测试 + release 构建 → Docker 镜像冒烟）。已推送并实际跑通诊断：修复触发器 `crates/insync-*` 单星号不跨 `/` 导致代码改动不触发、Linux 任务误构建 GPUI 导致 fontconfig 缺失、素材漂移字节比对跨平台不稳定、六个 crate 未继承 workspace 版本（违反 `test_product_version_contract`）
- [x] F3 本地 macOS 构建 + 冒烟（进程稳定 35s、干净退出、RSS 平稳 ~118MB、CPU ~6–8% debug 构建；全部 79 个测试通过；截图因测试显示器输出全黑无法目检，已如实记录。Windows 目标：六 crate `cargo xwin clippy --target x86_64-pc-windows-msvc -D warnings` 全绿，含 gpui-kit/tray-icon/muda/ring 路径）
- [x] F4 最终交付报告（docs/insync/final-report.md）

## 已知剩余工作

见 `docs/insync/final-report.md`「未完成与待办」一节（OS 原生通知、安装包流水线、
闲时素材精修、盘怪触发的端到端公网验证等）。
