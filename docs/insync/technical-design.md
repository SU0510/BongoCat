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

复用的 BongoCat 基础设施：`bongocat-storage`（原子私有写）、`bongocat-log`
（脱敏日志）、`bongocat-audio`（rodio 播放）、`bongocat-platform`（托盘/菜单、
开机自启、单实例、主题）、`bongocat-i18n`、gpui-kit。不使用：Live2D/Cubism、
全局输入监听、更新签名通道（见 ADR-0089）。

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

## 本地存储根

`<data_dir>/com.ayangweb.insync/{development,production}/`：
`account.json`、`settings.json`、`window-state.json`、`pet-state.json`、
`personal/{todos,notes,anniversaries,alarms,focus-history}.json`、
`sync/{session,outbox,message-cache,shared-cache}.json`、`backups/`、`logs/`。
全部走 schema_version 严格入口 + 原子写 + 损坏备份（`bongocat-storage` 原语）。

## 错误与恢复要点

- 网络断开：WS 自动重连（指数退避，上限 60s），outbox 保留重试；重要数据
  通过 REST 补拉，短期互动（戳戳）超时放弃并给轻量提示。
- 登录过期：UI 明确提示重新登录，不清本地缓存。
- 解除绑定：立即失去共享访问（服务端强制），本地按 `relation.dissolved`
  清空共享缓存；历史聊天保留在服务器至宽限期结束。
- 数据损坏：store 回退到最近有效备份或默认值，隔离损坏文件。
- 素材缺失：回退到 `idle` 动画并记录告警，不崩溃。
