# InSync TODO

每个任务先有自动化验证，再标记完成。完成后在括号内附验证命令与结果摘要。

## Phase A — 设计与素材（当前）

- [x] A1 源码审计 + 架构决策（ADR-0089/0090/0091、docs/insync/technical-design.md、protocol.md、sprite-assets.md）
- [ ] A2 素材生成管线：`tools/insync-sprites/{generate,validate,preview}_sprites.py`，13 个动画全绿（`validate_sprites.py` + 人工预览）
- [ ] A3 `insync-sprite`：manifest 加载、帧计算、优先级状态机、随机行为、单元测试

## Phase B — 桌宠窗口

- [ ] B1 `insync-pet-window` macOS NSPanel 后端（透明/置顶/拖动/点击/右键/隐藏）
- [ ] B2 `insync-pet-window` Windows 分层窗口后端（UpdateLayeredWindow、HTCAPTION 拖动、WM_CONTEXTMENU、DPI）
- [ ] B3 位置持久化、多屏钳制、缩放、开机位置恢复

## Phase C — 协议与服务端

- [ ] C1 `insync-core`：领域模型、协议 DTO、事件信封、outbox/幂等语义 + 单元测试
- [ ] C2 `insync-server`：auth（argon2+token）、配对/解除、消息/留言/戳戳（限频）
- [ ] C3 `insync-server`：专注状态机（revision 幂等）、共享待办/便签/纪念日、presence、事件日志
- [ ] C4 `insync-server` 集成测试：双用户绑定/消息/留言/戳戳/专注全流程/离线恢复/重复幂等

## Phase D — 客户端同步

- [ ] D1 `insync-client`：REST+WS、重连退避、补拉去重、presence（独立 tokio 线程）
- [ ] D2 安全测试：未登录/非成员/越权 ID/token 过期撤销/解绑后访问/日志隐私

## Phase E — 应用集成

- [ ] E1 `insync-app` 骨架：托盘、桌宠窗口集成、点击/戳戳反应、窗口状态持久化
- [ ] E2 本地 store：settings/account/personal/sync 全套 JSON store + 损坏恢复
- [ ] E3 聊天窗口 + 留言 + 戳戳面板 + 在线状态
- [ ] E4 共同专注面板（邀请/接受/倒计时/暂停/结束/勿扰）
- [ ] E5 共享待办/便签/纪念日面板 + 冲突处理（便签 revision 冲突可见）
- [ ] E6 时钟 + 闹钟（jiff、DST、错过触发恢复、声音）+ 通知中心（节流/勿扰）
- [ ] E7 设置面板（账号/服务器/桌宠/通知/语言/主题）+ 错误路径（断网/过期/解绑/素材缺失）

## Phase F — 交付

- [ ] F1 Dockerfile + compose + .env.example + deployment.md（备份/恢复）
- [ ] F2 CI workflow（Windows/macOS 构建 InSync 产物，本地不打包）
- [ ] F3 本地 macOS 构建 + 测试证据；Windows 交叉检查与未验证项说明
- [ ] F4 最终交付报告（docs/insync/final-report.md）

## 退出条件

- A/B：桌宠窗口在 macOS 实机显示原创角色动画，托盘/退出可用。
- C/D：服务端集成测试全绿（含安全矩阵）。
- E：`cargo test -p insync-core -p insync-server -p insync-sprite -p insync-client` 全绿；
  macOS 运行冒烟（桌宠 + 面板打开）。
- F：交付物路径明确，未实测项如实标注。
