# InSync 部署指南

服务端是一个单独的 Rust 二进制（`insync-server`），使用 SQLite（WAL）存储，
TLS 由反向代理终止。适合任何 1C/1GB 以上的 VPS 或容器环境。

## 1. 环境变量

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `INSYNC_DB` | `./insync.db` | SQLite 数据库文件路径（目录需可写） |
| `INSYNC_PORT` | `8787` | HTTP/WS 监听端口 |
| `INSYNC_TOKEN_TTL_DAYS` | `30` | 登录令牌有效期（天） |

没有任何密钥需要在代码或镜像中配置；令牌哈希与密码哈希都只存数据库。

## 2. Docker 部署（推荐）

```bash
docker compose up -d --build
# 服务监听容器内 8787，compose 已映射到宿主机 127.0.0.1:8787
```

数据（`insync.db`、WAL 文件）落在命名卷 `insync-data`，可用
`docker run --rm -v insync-data:/data alpine tar cz -C /data .` 备份。

容器以 `1000:1000` 运行，镜像内 `/data` 已预先创建并归属该用户（Docker 会把
镜像中该目录的属主复制进它创建的卷；若 `/data` 归 root，服务端会以
`open db: unable to open database file` 启动失败）。挂载自己的宿主机目录时
同样要保证该 UID 可写：

```bash
mkdir -p /srv/insync-data && sudo chown 1000:1000 /srv/insync-data
# docker-compose.yml 中改为: - /srv/insync-data:/data
```

镜像内已安装 `curl`，compose 的健康检查据此探测 `/health`。

### 2.1 裸二进制部署（从 Release 下载）

不使用 Docker 时，服务端二进制可以直接从 GitHub Release 取：

| 平台 | 资产 |
| --- | --- |
| Linux（最常见的 VPS 场景） | **没有预编译资产**：用 Docker（§2）或自行 `cargo build -p insync-server --release` |
| macOS 服务器（Apple Silicon） | `insync-server-macos-aarch64`（Intel Mac 需自行构建） |
| Windows 服务器 | `insync-server-windows-x86_64.exe` |
| macOS 桌宠（Apple Silicon） | `insync-app-macos-aarch64` |
| Windows 桌宠 | `insync-app-windows-x86_64.exe` |

Release 正文里的 `SHA256SUMS` 可与下载到的文件核对（`shasum -a 256 -c SHA256SUMS`）。
这些是**裸二进制**：macOS 版未签名、未公证（`xattr -d com.apple.quarantine` 或右键
「打开」），Windows 版可能触发 SmartScreen；版本号、标签与通道见
`docs/adr/0092-insync-versioning-and-release-channel.md`。服务器与桌面应用请成对升级
（协议版本由 `GET /version` 报告）。

## 3. 反向代理（HTTPS/WSS）

Caddy 示例（自动签发证书）：

```
insync.example.com {
    reverse_proxy 127.0.0.1:8787
}
```

nginx 示例：

```nginx
server {
    listen 443 ssl;
    server_name insync.example.com;
    ssl_certificate     /etc/letsencrypt/live/insync/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/insync/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8787;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;      # WebSocket
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_read_timeout 3600s;                    # WS 长连接
    }
}
```

客户端「设置 → 服务器地址」填 `https://insync.example.com`（或局域网
`http://<ip>:8787`）。

## 4. 备份与恢复

- **热备**：`sqlite3 insync.db ".backup '/backup/insync-$(date +%F).db'"`
  （WAL 模式下安全）。
- **冷备**：停服后拷贝 `insync.db`。
- **恢复**：停服 → 用备份覆盖 `INSYNC_DB` 路径 → 启动。数据库 schema 带
  版本号，旧版本文件由更新版本的服务端自动迁移；跨大版本降级不支持。

## 5. 健康检查

- `GET /health` → `ok`
- `GET /version` → `{"service":"insync-server","protocol_version":1}`

## 6. Windows 交叉构建验证（macOS/Linux 开发机）

在 macOS 上验证 InSync 全部 crate 的 Windows 目标可构建性（含 C 依赖 ring）：

```bash
cargo install cargo-xwin
brew install llvm   # 提供 clang-cl / llvm-lib
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
cargo xwin clippy --target x86_64-pc-windows-msvc \
  -p insync-core -p insync-server -p insync-client \
  -p insync-sprite -p insync-pet-window -p insync-app \
  --all-targets -- -D warnings
```

`cargo xwin` 自动下载 Windows SDK 头/库并用 clang-cl 编译 ring 的 C 代码；
只做编译检查不链接，无法替代 Windows 实机运行验证。

## 7. 客户端桌面应用

桌面应用的裸二进制（macOS aarch64 / Windows x86_64）从 GitHub Release 下载，资产名见
§2.1；本地开发用 `cargo run -p insync-app`。仍未接入安装包流水线（BongoCat 的
`bongocat-packaging` 不打 InSync 包，也没有签名/公证/自动更新），缺口清单见
`docs/insync/final-report.md` 的「未完成与待办」。

## 8. 安全清单

- [x] 密码 Argon2id 哈希；令牌仅存 SHA-256
- [x] 全部关系数据经服务端鉴权（见 `crates/insync-server/tests/integration.rs`
      的 `security_matrix`）
- [x] 限流：登录失败 10/15min、注册 10/h、邀请 5/day、戳戳 2s 冷却、消息 20/min
- [x] 数据库文件/会话文件 0600 私有权限
- [ ] 反向代理层 TLS（部署时配置）
- [ ] 数据库磁盘加密（按需，如 LUKS）
