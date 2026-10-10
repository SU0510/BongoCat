# InSync 实时协议 v1

服务端与客户端共享 `insync-core` 中的类型定义，本文是人读规范。所有时间戳为
Unix epoch 毫秒（i64）。所有客户端生成的对象携带 `client_id`（UUIDv7 字符串），
服务端以 `(relation_id, client_id)` 唯一索引实现幂等：重试不产生重复数据。

## 认证

- `POST /v1/auth/register` `{username, password, display_name}` → `201 {user, token, expires_at}`。
- `POST /v1/auth/login` `{username, password}` → `200 {user, token, expires_at}`。
- `POST /v1/auth/refresh`（Bearer）→ `200 {token, expires_at}`（轮换，旧 token 立即失效）。
- `POST /v1/auth/logout`（Bearer）→ 撤销当前 token。
- Token 为 256-bit 随机值的 base64url；服务端只存 SHA-256；有效期 30 天，可撤销。
- 密码使用 Argon2id（PHC 编码）哈希。错误响应统一 `{"error": {"code": "...", "message": "..."}}`。

## 配对（伴侣关系）

- `POST /v1/pair/invite` → `{code, expires_at}`（8 位 base32，24h 有效，单次使用；
  已有 active 关系的用户不能创建；每用户每日 ≤ 5 个）。
- `POST /v1/pair/accept` `{code}` → `{relation, partner}`。
- `POST /v1/pair/dissolve` → 关系标记 dissolved；双方立即失去共享访问
  （服务端强制 403 `RELATION_INACTIVE`），广播 `relation.dissolved`；
  共享数据保留 30 天后由服务端维护任务清除。
- `GET /v1/me` → `{user, relation?, partner?, presence}`。

Relation 模型：`{id, user_a, user_b, status: active|dissolved, created_at,
last_interaction_at, dissolved_at?}`。一个用户最多一个 active 关系。

## 消息（实时聊天）

- `POST /v1/messages` `{client_id, body}` → `{message}`。body ≤ 2000 字符。
- `GET /v1/messages?after_seq=<seq>&limit<=200` → `{messages, has_more}`（按 seq 升序）。
- `POST /v1/messages/read` `{last_message_seq}` → 204。双方各自维护已读水位。
- Message：`{seq, id, sender_id, body, created_at}`。`seq` 为关系内单调递增。
- 未读数 = 对方消息 max(seq) − 我的已读水位，由 `GET /v1/overview` 一次给出。

## 留言（异步）

- `POST /v1/letters` `{client_id, body}`；`GET /v1/letters?unread=true|all`；
- `POST /v1/letters/{id}/read`；`DELETE /v1/letters/{id}`（仅发送方可删除）。
- Letter：`{id, sender_id, body, created_at, read_at?}`。上线时 `overview`
  返回未读留言数；不重放过期动画。

## 戳戳

- `POST /v1/pokes` `{client_id}` → `{poke}`；服务端限频：同一发送者 2s 内 1 次
  （429 `RATE_LIMITED` 带 `retry_after_ms`），事件保留 7 天。
- 接收方离线时事件照常入库；重新上线通过 `overview.unread_pokes`（≤3 的计数）
  给一条轻量提醒，不重放动画。
- 本地反馈立即发生（本地戳出动画），失败（429/断网）给轻量提示。

## 共同专注（服务端权威状态机）

FocusSession：`{id, relation_id, initiator_id, topic?, duration_secs,
state, revision, invited_at, started_at?, end_at?, paused_at?, paused_ms,
ended_at?, ended_by?}`。`revision` 随每次状态转移单调递增；客户端按 revision
幂等应用。

状态机：`invited → active → paused ⇄ active → completed | cancelled |
declined`。转移规则：

- `POST /v1/focus/invite` `{client_id, topic?, duration_secs(60..=7200)}` → `invited`。
  对方 10 分钟未响应自动 `cancelled`。
- `POST /v1/focus/{id}/accept`（受邀方）→ 双方都已接受则立即 `active`：
  `started_at = now`，`end_at = now + duration`。 → 广播 `focus.started`。
- `POST /v1/focus/{id}/decline` → `declined`。
- `POST /v1/focus/{id}/cancel`（发起方，未开始时）→ `cancelled`。
- `POST /v1/focus/{id}/pause`（任一方，active）→ `paused`，记录 `paused_at`。
- `POST /v1/focus/{id}/resume`（任一方，paused）→ `active`，`paused_ms` 累加，
  `end_at` 顺延。→ 广播 `focus.resumed`。
- `POST /v1/focus/{id}/finish-early`（任一方，active/paused）→ `completed`（early）。
- 自然结束：服务端维护任务在 `now ≥ end_at` 时置 `completed` 并广播
  `focus.completed`（状态守卫保证只发一次）。客户端本地到点后先渲染庆祝，
  以服务端 `focus.completed` 事件做一次性确认；断线恢复以服务端状态为准。
- 会话期间 `presence` 携带 `focus_session_id`，对方可见"正在共同专注"。

## 共享待办 / 便签 / 纪念日

- Todo：`{id, creator_id, title, detail?, priority: low|normal|high,
  due_at?, completed_at?, completed_by?, created_at, updated_at, deleted_at?}`。
  `POST/PATCH/DELETE /v1/todos`，PATCH 幂等（整对象 + updated_at 服务端生成）。
- Note：`{id, author_id, content, revision, updated_by, updated_at}`。
  `PATCH` 携带 `base_revision`，过期返回 `409 {current}`，由用户决定覆盖或放弃，
  不静默丢弃任何一方内容。
- Anniversary：`{id, creator_id, name, date(YYYY-MM-DD), repeat_yearly, note?,
  created_at, updated_at, deleted_at?}`。
- 以上全部仅关系双方可读写（服务端逐请求校验）；个人版本只存本地。

## 在线状态

- Presence：`{user_id, state: online|away|dnd, last_seen_at, focus_session_id?}`。
- 连接建立即 online；WS 断开后 30s 置 offline 并广播 `presence.updated`；
  `away/dnd` 由用户设置（`POST /v1/presence`）。心跳：客户端每 25s 发应用层
  `{type:"ping"}`，服务端回 `{type:"pong"}`；服务端每 30s 发 WS protocol ping。
- 在线状态展示可整体关闭（客户端设置，只影响本地显示）。

## 事件信封与 WebSocket

`WS /v1/ws`（`Authorization: Bearer` 头或 `?token=` 查询参数）：

```json
{"seq": 128, "id": "0190...", "type": "message.created", "actor_id": "u1",
 "relation_id": "r1", "server_time": 1760000000000, "version": 1,
 "data": { "message": { ... } }}
```

握手：客户端发 `{"type":"hello","last_seq":127}` → 服务端回
`{"type":"ready","latest_seq":128}`，然后推送 `seq > last_seq` 的日志事件；
`last_seq` 缺失时只推新事件（历史走 REST 补拉）。事件类型：

`message.created, message.read, letter.created, letter.read, poke.received,
focus.invited, focus.accepted, focus.declined, focus.cancelled, focus.started,
focus.paused, focus.resumed, focus.completed, todo.created, todo.updated,
todo.completed, todo.deleted, note.created, note.updated, note.deleted,
anniversary.created, anniversary.updated, anniversary.deleted,
presence.updated, relation.dissolved, invite.accepted`

- 事件先持久化到 `events(seq AUTOINCREMENT, relation_id, id UNIQUE, type,
  actor_id, payload, server_time)` 再推送；`GET /v1/events?after=<seq>&limit<=500`
  补拉。事件保留 7 天（pokes/presence 类 24h）；长期恢复一律走各资源 REST。
- 客户端对未知 `type` 跳过（向前兼容）；同一 `(type, id)` 只处理一次（本地
  去重表）。

## 限流与滥用防护

per-user+IP：register 10/h，login 失败 10/15min，invite 5/day，pokes 2s 冷却，
messages 20/min。超限返回 429 + `retry_after_ms`。
