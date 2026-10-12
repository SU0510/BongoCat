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

**Nana**：Q 版**大头短腿**比例（头占身高近一半、躯干短、腿短鞋厚，接近二次元
游戏常见的 chibi 头身），按真实人物特征 redesign：深棕色双麻花辫（贴脸而下、
搭在肩膀**身前**垂到发绳，含发绳与行走摆动）+ 玫瑰粉无袖上衣 + 斜挎包带 +
瓜子脸（脸颊到眼下仍然饱满，随后五行短收到一个小下巴，不做刻意尖下巴）；
头发**贴着发际线向后梳**，没有刘海——发团从 y5 一路盖到 y19，发际线是额中
x21..26 开口、再向下逐渐加宽的一道窄拱，额前只留 2–3 行皮肤；y20 起头发在
两侧各收成 3 像素鬓发（x13..15 / x32..34），把脸框在 16 像素宽的窗口里，再沿
下颌汇入辫子；中分处一笔 `hair_shadow` 读出分缝。无呆毛。眼睛是 3×4 的竖长
杏眼、上排两像素高光，腮红在 y25、嘴在 y27。第一版肤色/描边直接取照片色，
观感发黄发黑；已提亮为暖 ivory 肤色 + 软暖褐描边。形状与 Momo 明显不同（大头
比例、辫子垂在胸前、无抽绳与口袋、领口为弧形），因此不适合用调色板变体表达。

**结构旗标**：绘制函数按 `Style` 旗标（`braids` / `combed_back` /
`sleeveless` / `strap` / `big_head` / `ahoge`）分支，而不是按角色名分支——
新增调色板变体零代码，新增结构特征时在 `STYLES` 里声明旗标即可
（`tools/insync-sprites/generate_sprites.py`）。共享的绘制规则：辫子在躯干
之后、手臂与包带之前渲染（举臂时手臂在前、不穿模；包盖住发梢），贴脸而下、
搭肩垂到发绳，行走循环 ±1px 摆动、发绳用角色 accent 色，辫身每两行向外鼓一
像素读出麻花的阶梯感。

`big_head` 是唯一的比例旗标：头部（含发团轮廓）从 y5 到下颌 y31（更大的头骨 +
瓜子脸收窄 + 更大更圆的杏眼），躯干压缩到 y32..45，腿变短、鞋加厚，但**落地线
与 Momo 保持一致**（脚底描边同样在 y59）——桌宠窗口固定 192×256，两个角色必须
站在同一条地平线上，否则切角色时会显得悬空。`big_head` 目前与 `sleeveless`
搭配（只有 Nana）：chibi 分支只画无袖领口，帽领/抽绳/口袋仍走 Momo 路径。

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
| 描边 | `#6b4f4a` |
| 皮肤 | `#ffe3c9` / 阴影 `#f3c7a1` |
| 头发主色 | `#7a5c50` / 阴影 `#523a34` / 高光 `#a07d68` |
| 上衣 | `#f2b7bd` / 阴影 `#dba0a8` / 包底 `#c48892` |
| 裤子 | `#6f89b8` / 阴影 `#54709f` |
| 发绳/accent | `#ef7280` |
| 腮红 | `#f79fa4` |
| 眼睛 | `#5d443f` / 高光白 `#ffffff` |

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
   （如 nana 的 `braids` / `combed_back` / `big_head`），绘制函数按旗标分支。
2. 运行 `python3 tools/insync-sprites/generate_sprites.py` 重新生成并核对
   `manifest.json` 新增该角色；`validate_sprites.py` 对 manifest 里的每个角色
   自动执行同样的校验。
3. `tools/insync-ui/generate_ui_images.py` 的 `CHARACTERS` 加入该角色，
   `PORTRAIT_CROP` 加该角色的头像裁切窗口（比例不同的角色必须有各自的窗口：
   chibi 头更高，沿用默认窗口会切掉下巴），重新生成
   `{character}_portrait.png` / `{character}_wave.png`。
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

## 面板视觉验收（真机，用 CI 产物）

`verify_panel.py` 需要真实 macOS 图形会话：用 Quartz 定位 `insync-app` 的面板窗口，
再以 `screencapture -l <窗口ID>` 截取该窗口并做像素/几何测量。CI runner 没有交互式
图形会话，也没有屏幕录制授权，这一步只能在开发机上执行——但**不需要本地编译**，
直接下载 CI 已构建的产物：

```
gh run list --workflow=insync-verify.yml                # 找到待验收 commit 对应的 run
gh run download <run-id> -n insync-macos-aarch64 -D /tmp/insync-ci
python3 tools/insync-ui/verify_panel.py --app /tmp/insync-ci/insync-app --all /tmp/insync-panels
```

- 产物必须来自与待验收代码相同的 commit，不允许拿旧产物验收新改动。
- 终端需要屏幕录制权限（系统设置 → 隐私与安全性），否则 `screencapture` 失败或输出全黑。
- 素材生成与漂移门禁（`generate_sprites.py`、`generate_ui_images.py --check`）不依赖
  这一步，也不需要任何编译；验收面板外观时不要再在本地 `cargo build`（编译与测试由
  CI 覆盖）。

## 禁止

- 使用来源不明的网络素材或受版权保护资产。
- 绕过校验脚本直接改动 PNG。
- 在帧内引入调色板之外的颜色。
