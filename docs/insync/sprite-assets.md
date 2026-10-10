# InSync 像素角色素材规范

## 目录

```
resources/insync/sprites/
  manifest.json            # 动画清单（由生成器输出，运行时唯一事实来源）
  momo/<animation>.png     # 每个动作一张横向精灵图（等宽帧）
tools/insync-sprites/
  generate_sprites.py      # 程序化像素素材生成器（PIL，可重复运行）
  validate_sprites.py      # 尺寸/帧数/透明通道/调色板一致性校验
  preview_sprites.py       # 生成 GIF 预览（供人工检查循环与闪烁）
```

## 角色设定

**Momo**：Q 版 2.5 头身、温暖治愈系日系像素风。桃粉色波波头 + 奶油色连帽衫 +
深暖灰描边；两套调色板变体（`momo` 桃粉 / `momo-mint` 薄荷），形状完全一致，
便于两位伴侣区分。逻辑分辨率 48×64，输出 4 倍放大（192×256/帧），
nearest-neighbor 保证像素锐利。

调色板（全局固定，跨帧一致）：

| 用途 | 色值 |
| --- | --- |
| 描边 | `#4a3b forty`→ `#4a3b3b` |
| 皮肤 | `#f7d7b8` / 阴影 `#eab890` |
| 头发主色 | `#f2a6b3` / 阴影 `#d97f92` / 高光 `#ffd3dc` |
| 连帽衫 | `#fff4e3` / 阴影 `#e8d5b8` / 抽绳 `#f2a6b3` |
| 裤子 | `#7fb5a3` / 阴影 `#5f9483` |
| 腮红 | `#f6b1b1` |
| 眼睛 | `#4a3b3b` / 高光白 `#ffffff` |

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

## 新增动作流程

1. 在 `generate_sprites.py` 的 `ANIMATIONS` 中定义帧参数（部位偏移/表情/道具）。
2. 运行 `python3 tools/insync-sprites/generate_sprites.py` 重新生成。
3. `python3 tools/insync-sprites/validate_sprites.py` 必须全绿：画布尺寸一致、
   帧数正确、无帧全透明、调色板仅含声明颜色、主体未越过画布边界、循环动画
   首尾帧不跳变（容差内）。
4. `python3 tools/insync-sprites/preview_sprites.py` 输出 GIF 至
   `target/insync-preview/`，人工检查循环自然、无闪烁、无白边。
5. 运行时（`insync-sprite`）加载 manifest；单个动画缺失回退 `idle`，不崩溃。

## 禁止

- 使用来源不明的网络素材或受版权保护资产。
- 绕过校验脚本直接改动 PNG。
- 在帧内引入调色板之外的颜色。
