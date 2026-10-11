# InSync 像素角色素材规范

## 目录

```
resources/insync/sprites/
  manifest.json            # 动画清单（由生成器输出，运行时唯一事实来源）
  <character>/<animation>.png   # 每个角色每个动作一张横向精灵图（等宽帧）
tools/insync-sprites/
  generate_sprites.py      # 程序化像素素材生成器（PIL，可重复运行）
  validate_sprites.py      # 尺寸/帧数/透明通道/调色板一致性校验
  preview_sprites.py       # 生成 GIF 预览（供人工检查循环与闪烁）
```

## 角色设定

**Momo**：Q 版 2.5 头身、温暖治愈系日系像素风。桃粉色波波头 + 奶油色连帽衫 +
深暖灰描边；两套调色板变体（`momo` 桃粉 / `momo-mint` 薄荷），形状完全一致，
便于两位伴侣区分。

**Nana**：同一体型与绘制管线，按真实人物特征 redesign：深棕色双麻花辫（含
中分空气刘海、发绳与行走摆动）+ 玫瑰粉无袖上衣 + 斜挎包带，无呆毛。形状与
Momo 明显不同（辫子在裸臂后层、无抽绳与口袋、领口为弧形），因此不适合用
调色板变体表达。

**结构旗标**：绘制函数按 `Style` 旗标（`braids` / `center_part` /
`sleeveless` / `strap` / `ahoge`）分支，而不是按角色名分支——新增调色板
变体零代码，新增结构特征时在 `STYLES` 里声明旗标即可（`tools/insync-
sprites/generate_sprites.py`）。共享的绘制规则：辫子画在皮肤后层（裸臂与手
覆盖辫子，不穿模）、行走循环 ±1px 摆动、发绳用角色 accent 色。

逻辑分辨率 48×64，输出 4 倍放大（192×256/帧），
nearest-neighbor 保证像素锐利。

调色板按角色声明（生成器 `PALETTES`，全局固定，跨帧一致）。

**Momo**（`momo` 与 `momo-mint` 共享形状，仅换色）：

| 用途 | 色值 |
| --- | --- |
| 描边 | `#4a3b3b` |
| 皮肤 | `#f7d7b8` / 阴影 `#eab890` |
| 头发主色 | `#f2a6b3` / 阴影 `#d97f92` / 高光 `#ffd3dc` |
| 连帽衫 | `#fff4e3` / 阴影 `#e8d5b8` / 抽绳 `#f2a6b3` |
| 裤子 | `#7fb5a3` / 阴影 `#5f9483` |
| 腮红 | `#f6b1b1` |
| 眼睛 | `#4a3b3b` / 高光白 `#ffffff` |

**Nana**（另有 `strap` 斜挎包带 `#4a4038`、`shoe` `#e8e2d6`）：

| 用途 | 色值 |
| --- | --- |
| 描边 | `#3d2c29` |
| 皮肤 | `#f7d7b8` / 阴影 `#eab890` |
| 头发主色 | `#6b5147` / 阴影 `#453029` / 高光 `#8d6e58` |
| 上衣 | `#e3a6ab` / 阴影 `#c98b93` / 包底 `#b57b85` |
| 裤子 | `#6f89b8` / 阴影 `#54709f` |
| 发绳/accent | `#e0636f` |
| 腮红 | `#f6b1b1` |
| 眼睛 | `#3d2c29` / 高光白 `#ffffff` |

## 动画清单（v1，13 个）

| 名称 | 帧数 | fps | 循环 | 说明 |
| --- | --- | --- | --- | --- |
| idle | 4 | 3 | loop | 呼吸起伏（身体 1px 浮动） |
| blink | 4 | 8 | once→idle | 眨眼 |
| wave | 6 | 6 | once | 抬手挥手 |
| walk | 6 | 8 | loop | 双腿交替 + 身体起伏 |
| sleep | 4 | 2 | loop | 闭眼歪头，Zzz 气泡 |
| work | 4 | 3 | loop | 面前小本本，笔尖点动 |
| stretch | 6 | 5 | once | 双臂上举伸展 |
| react | 4 | 8 | once | 惊讶（!气泡，眼睛睁大） |
| poke_react | 6 | 8 | once | 被戳：后仰 + 爱心/星星 |
| message_react | 4 | 6 | once | 看信封，头上气泡 |
| invite_react | 4 | 6 | once | 举沙漏/时钟，点头 |
| celebrate | 6 | 8 | once | 双手举起跳跃，彩纸 |
| rest | 4 | 3 | loop | 盘坐放松，茶杯热气 |

不同动作必须有可见差异（姿态/表情/道具/位移），不允许只改名字。

## manifest.json 格式

```json
{
  "format_version": 1,
  "characters": {
    "momo": {
      "frame_width": 192, "frame_height": 256,
      "animations": {
        "idle": {"file": "momo/idle.png", "frames": 4, "fps": 3, "loop": true}
      }
    }
  }
}
```

## 新增角色流程

1. 在 `generate_sprites.py` 的 `PALETTES` 声明调色板；形状与现有角色一致时
   只需调色板（如 `momo-mint`），有结构性差异时在 `STYLES` 声明 `Style` 旗标
   （如 nana 的 `braids` / `strap`），绘制函数按旗标分支。
2. 运行 `python3 tools/insync-sprites/generate_sprites.py` 重新生成并核对
   `manifest.json` 新增该角色；`validate_sprites.py` 对 manifest 里的每个角色
   自动执行同样的校验。
3. `tools/insync-ui/generate_ui_images.py` 的 `CHARACTERS` 加入该角色，重新
   生成 `{character}_portrait.png` / `{character}_wave.png`。
4. `insync-app`：`store::CHARACTERS` 加入角色 id、`assets.rs` 的 `sprite_library`
   用 `sheets!` 内嵌该角色的 13 张表（`from_embedded` 要求 manifest 中每个角色
   的表都在，缺一张即 panic）、`i18n.rs` 加 `character.<id>` 文案。旧数据兼容
   由 `#[serde(default)]` + validate 回退首个角色保证，并补一条「旧文件无该
   字段 → 正常启动」回归测试。
5. 面板头像与空态插图按 `insync/{character}_portrait.png` 跟随当前角色。

## 新增动作流程

1. 在 `generate_sprites.py` 的 `ANIMATIONS` 中定义帧参数（部位偏移/表情/道具）。
2. 运行 `python3 tools/insync-sprites/generate_sprites.py` 重新生成。
3. `python3 tools/insync-sprites/validate_sprites.py` 必须全绿：画布尺寸一致、
   帧数正确、无帧全透明、调色板仅含声明颜色、主体未越过画布边界、循环动画
   首尾帧不跳变（容差内）。
4. `python3 tools/insync-sprites/preview_sprites.py` 输出 GIF 至
   `target/insync-preview/`，人工检查循环自然、无闪烁、无白边。
5. 运行时（`insync-sprite`）加载 manifest；单个动画缺失回退 `idle`，不崩溃。

## 面板用静态裁切图

面板（页头头像、空态插图）复用同一张精灵图，而不是另画一套角色，避免两处形象
漂移：

```
resources/insync/ui/
  <character>_portrait.png # 从 idle 帧裁出的头像（红色心形装饰，页头用）
  <character>_wave.png     # wave 动作第 1 帧的整幅（空态插图用）
tools/insync-ui/
  generate_ui_images.py    # 由 sprites 生成上述两张图（PIL，可重复运行）
  verify_panel.py          # 截取真实面板窗口并做像素/几何测量（人工目检的替代）
```

每个可用角色各生成一对裁切图；`insync-app` 以 `include_bytes!` 内嵌全部角色的
两张图（`InSyncAssets`），运行时按 `insync/{character}_portrait.png` 取当前
角色对应的图，因此精灵图重绘后重新运行生成器即可同步，运行时不需要文件路径。

与精灵图同理，这两张图也必须与生成器输出一致：CI 的 `assets` 任务运行
`generate_ui_images.py --check`（内存重新生成后比对**解码像素**，不是字节——zlib
编码器在 macOS 与 Ubuntu 间不同，像素才是面板真正绘制的东西），漂移即失败。

## 禁止

- 使用来源不明的网络素材或受版权保护资产。
- 绕过校验脚本直接改动 PNG。
- 在帧内引入调色板之外的颜色。
