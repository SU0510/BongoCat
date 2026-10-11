# InSync 技术设计

InSync 是面向情侣（尤其异地情侣）的 Windows/macOS 桌面陪伴应用：原创像素桌宠 +
双人实时互动（消息、留言、戳戳、共同专注）+ 共享日常工具（待办、便签、纪念日、
闹钟、时钟）。本文只描述当前目标架构；决策背景见 `docs/adr/0089`、`0090`、`0091`。

## 模块边界

| 模块 | crate / 位置 | 职责 |
| --- | --- | --- |
| Pet Renderer + Behavior | `crates/insync-sprite` | 精灵图清单、帧计算、动画优先级状态机、随机生活动作 |
| Desktop Shell（桌宠窗口） | `crates/insync-pet-window` | 透明置顶窗口、拖动、点击、穿透、DPI、多屏（macOS NSPanel / Windows 分层窗口） |
| UI | `crates/insync-app`（GPUI） | 聊天窗口、功能面板（TabBar hub）、托盘、设置 |
| Local Store | `crates/insync-app`（`local_store` 模块） | 版本化 JSON 存储：账号/设置/个人数据/缓存/离线队列，原子写 + 备份 |
| Sync Engine + Realtime Client | `crates/insync-client` | REST/WS 客户端、outbox、重连退避、补拉、去重、在线状态（独立线程 + tokio） |
| 协议 | `crates/insync-core` | 领域类型、事件协议 v1、幂等键、离线语义（server/client 共享） |
| Backend API + Realtime Server | `crates/insync-server` | 认证、配对、消息/留言/戳戳/专注/共享数据、事件日志、WS 推送 |
| Database | `insync-server`（SQLite WAL） | users、sessions、relations、messages、letters、pokes、focus、todos、notes、anniversaries、events、presence |
| Notification Service | `insync-app` | 应用内通知中心 + 声音（`bongocat-audio`）、勿扰、节流 |
| Chime Asset | `insync-app` + `tools/insync-audio` | 程序化生成的 0.9s 双音铃声（WAV），`include_bytes!` 内嵌，首次使用时落到私有数据根 |

复用的 BongoCat 基础设施：`bongocat-storage`（原子私有写）、`bongocat-log`
（脱敏日志）、`bongocat-audio`（rodio 播放）、`bongocat-platform`（托盘/菜单、
开机自启、单实例、主题）、`bongocat-i18n`、gpui-kit。不使用：Live2D/Cubism、
全局输入监听、更新签名通道（见 ADR-0089）。

`bongocat-audio` 的两条契约决定了铃声的接法：播放按路径取已解码的片段
（`Prepare` → `Play`），因此提醒音在启动时 `Prepare` 一次，否则 `Play` 会以
`ResourceIo` 静默失败；解码器只认 FLAC 与（本 ADR 为 InSync 新启用的）WAV，
所以铃声以确定性的纯 stdlib 生成器产出 WAV（`tools/insync-audio/`）。

## 数据流

```
[桌宠窗口] ←帧— insync-sprite 播放器 ←状态— InSyncCoordinator(主线程)
     ↑点击/拖动                                    ↑ 快照/命令 (channel)
[GPUI 面板: 聊天/hub/设置] ──────────────────────────┘
     ↓ 命令                                    快照(cell + 通知)
[insync-client 同步线程: outbox → REST/WS → 补拉/去重] ⇄ [insync-server]
                                                          ↓
                                                     SQLite (WAL)
```

- 桌宠本地反应（点击、戳出）立即发生，不等待网络；网络发送进 outbox。
- 服务端是共享数据与专注会话状态的唯一事实来源；本地倒计时按共享
  `started_at/end_at` 计算，重连后以服务端状态收敛。
- 个人数据（个人待办、个人便签、私人闹钟）只存在本地，永不共享。

## 窗口可见性契约

两条只有真实桌面才能暴露的约束，实现时不得省略：

- **桌宠窗口透明**：不仅要声明窗口不透明性为假，还必须把窗口背景色设为透明。
  macOS 上 `setOpaque(false)` 只表示「本窗口不是不透明」，AppKit 仍会用
  `backgroundColor`（默认 `windowBackgroundColor`）填满整个 frame，桌宠会站在一块
  实色矩形上；Windows 侧以 `UpdateLayeredWindow` + 逐像素 alpha 呈现。
- **显示面板 = 打开 + 提升**：InSync 是 accessory（无 Dock 图标）应用，自身从不成为
  活跃应用，而**未激活应用的窗口会被排到所有其它应用窗口之后**；桌宠又是浮动层级
  面板，始终盖在普通窗口之上——只创建窗口而不提升，用户看到的就是「只有桌宠，菜单里
  的打开面板毫无反应」。因此显示面板必须同时激活应用并把窗口提到最前；菜单项语义为
  「打开」（永不关闭），桌宠右键为「已在最前则关闭，否则提升」。

## 本地存储根

`<data_dir>/com.ayangweb.insync/{development,production}/`：
`account.json`、`settings.json`、`window-state.json`、`pet-state.json`、
`personal/{todos,notes,anniversaries,alarms,focus-history}.json`、
`sync/{session,outbox,message-cache,shared-cache}.json`、`assets/alarm.wav`
（内嵌铃声的落地副本）、`backups/`、`logs/`。
全部走 schema_version 严格入口 + 原子写 + 损坏备份（`bongocat-storage` 原语）。

## 错误与恢复要点

- 网络断开：WS 自动重连（指数退避，上限 60s），outbox 保留重试；重要数据
  通过 REST 补拉，短期互动（戳戳）超时放弃并给轻量提示。
- 登录过期：UI 明确提示重新登录，不清本地缓存。
- 解除绑定：立即失去共享访问（服务端强制），本地按 `relation.dissolved`
  清空共享缓存；历史聊天保留在服务器至宽限期结束。
- 数据损坏：store 回退到最近有效备份或默认值，隔离损坏文件。
- 素材缺失：回退到 `idle` 动画并记录告警，不崩溃。
