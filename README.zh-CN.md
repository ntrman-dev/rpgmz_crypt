# RPG Maker MZ / MV 自定义 JSON 数据加解密工具

[English documentation](./README.md)

本工具适用于这样一类 RPG Maker 游戏：`data/*.json` 不是明文，而是被包装成 `{"uid","bid","data"}`，并在管理器脚本加载时解密。


御爱论坛帖子： [RPG Maker MZ / MV data JSON 数据加解密工具](https://www.ai2.moe/topic/46675-rpg-maker-mzmv-%E5%8A%A0%E5%AF%86data%E6%95%B0%E6%8D%AE%E7%9A%84%E8%A7%A3%E5%AF%86%E5%B7%A5%E5%85%B7%E5%AE%9E%E7%8E%B0/)

有问题论坛私信，这个github账号不咋用

## 支持的游戏家族

目前支持并已验证的家族：

- 通过 `js/rmmz_managers.js` 加载这类加密 JSON 数据的 RPG Maker MZ 游戏
- 把同一家族 `uid/bid/data` JSON 包装逻辑移植到 `js/rpg_managers.js` 的自定义 RPG Maker MV 游戏

这和原版 RPG Maker MV 不同。原版 MV 的 `Decrypter` 主要处理图片和音频，`data/*.json` 通常仍然是明文 JSON，不使用这套 MZ 风格的数据包装。也就是说，是否支持主要看 `js/*managers.js` 里是否有这套 JSON 数据加载/解密逻辑，而不是只看名字写着 MZ 还是 MV。

## `/path/to/game` 需要包含什么

可用的游戏根目录必须包含 `data/`，以及对应引擎家族的管理器脚本：

- MZ 根目录：`data/` 和 `js/rmmz_managers.js`
- MV 自定义根目录：`data/` 和 `js/rpg_managers.js`

示例：

```text
/path/to/game/
├── data/
└── js/
    └── rmmz_managers.js
```

```text
/path/to/game/
├── data/
└── js/
    └── rpg_managers.js
```

## 自动检测与 `--game`

各个转换命令会先尝试自动检测游戏根目录。

- 如果输入路径和输出路径都在游戏目录树内，通常会自动检测成功。
- 如果输入路径或输出路径在游戏目录树外，就传 `--game /path/to/game`。
- 即使自动检测能成功，如果你想手动指定目标游戏，也可以显式传 `--game` 覆盖检测结果。

当前行为是：先自动检测，只有检测失败时，或者你想手动覆盖检测结果时，才需要 `--game`。

## 工具名称与兼容入口

- Python 主入口：`rpgdata_crypt.py`
- 向后兼容的 Python 别名：`rpgmz_crypt.py`
- Rust 二进制名：`rpgdata_crypt`
- Rust 源码目录：`rpgmz_crypt_rs/`

也就是说，仓库里还保留历史目录名 `rpgmz_crypt_rs/`，但对用户暴露的工具名现在统一是 `rpgdata_crypt`。

## 这类 JSON 加密的工作原理

### 1. 文件不是明文 JSON，而是包装 JSON

支持的游戏通常把每个加密数据文件存成这样的包装结构：

```json
{
  "uid": "...",
  "bid": "...",
  "data": "base64 ciphertext"
}
```

- `uid` 是游戏携带的标识符，本身不参与密钥推导
- `bid` 在补丁后的 loader 中可作为加密/明文区分标记
- `data` 才是真正的 base64 密文载荷

### 2. 参数来自引擎脚本，而不是来自数据文件本身

工具会从游戏根目录里的管理器脚本提取参数：

- MZ：`js/rmmz_managers.js`
- MV 自定义：`js/rpg_managers.js`

这就是为什么自动检测和 `--game` 很重要：单看加密 JSON 文件本身，并不知道该使用哪组 `_K`、XOR 常量、位移参数，或者文件名是否需要先转小写。

### 3. 文件名本身参与密钥推导

例如 `Map002.json` 的 `Map002` 会参与哈希；有些游戏会先转小写，有些不会。也就是说，文件名不只是展示用，而是密钥推导的一部分。

这就是为什么你不能把文件改名成 `Map002_edit.json` 之后再加密回去。

### 4. 解密是“从后往前 + 明文反馈”的 XOR 流

支持的这类算法核心是：

- 从文件末尾往开头处理
- 每个位置的 key byte 都和 `_K`、文件名哈希、位移/XOR 常量、当前位置索引、前一个明文字节有关
- 再用这个 key byte 去 XOR 当前密文字节

所以工具必须精确复现引擎里的 JS 整数行为，不能只写一个“差不多”的异或脚本。

### 5. `restore` 实际改了什么

`restore` 不只是把 `data/*.json` 解密出来，它还会：

1. 把原始加密 `data/` 备份到 `data.encrypted/`
2. 修改管理器脚本，让 loader 变成类似：

```javascript
if (c.bid) {
    // 加密包装 JSON -> 先解密
} else {
    // 明文 JSON -> 直接使用
}
```

正是这个 JS 补丁，让你在 `restore` 之后可以直接编辑明文 JSON，而不必每改一次就重新加密。

## 快速开始

### MZ 的 restore/revert 流程

如果你希望游戏直接运行明文 JSON，推荐使用：

```bash
python3 rpgdata_crypt.py restore /path/to/game
# 直接编辑 data/*.json
python3 rpgdata_crypt.py revert /path/to/game
```

MZ 的 `restore` 会做三件事：

1. 把加密数据备份到 `data.encrypted/`
2. 把 `data/*.json` 解密成明文 JSON
3. 修改 `js/rmmz_managers.js`，让游戏同时接受加密包装 JSON 和明文 JSON

`revert` 会恢复原始加密数据和 JS 备份。

### MV 自定义游戏的 restore/revert 说明

使用 `js/rpg_managers.js` 的 MV 自定义游戏同样支持这套便捷流程：

```bash
python3 rpgdata_crypt.py restore /path/to/game
# 直接编辑 data/*.json
python3 rpgdata_crypt.py revert /path/to/game
```

对于 MV 自定义游戏，`restore` 会做三件事：

1. 把加密数据备份到 `data.encrypted/`
2. 把 `data/*.json` 解密成明文 JSON
3. 修改 `js/rpg_managers.js`，让游戏同时接受加密包装 JSON 和明文 JSON

`revert` 会恢复原始加密数据和 JS 备份。

下面的传统转换流程仍然可用，适合你想做目录级解密/再加密往返，尤其是在游戏目录树外工作时。

## 传统转换流程

```bash
# 批量解密
python3 rpgdata_crypt.py decrypt /path/to/game/data ./data_plain --pretty

# 重新加密
python3 rpgdata_crypt.py encrypt ./data_plain /path/to/game/data

# 当路径不在游戏目录树内时，显式指定游戏根目录
python3 rpgdata_crypt.py decrypt /encrypted/data ./data_plain --pretty --game /path/to/game
python3 rpgdata_crypt.py encrypt ./data_plain /encrypted/data --game /path/to/game
```

单文件命令同样遵循这个规则：

```bash
python3 rpgdata_crypt.py decrypt-file /path/to/game/data/Map002.json ./Map002.json --pretty
python3 rpgdata_crypt.py encrypt-file ./Map002.json /path/to/game/data/Map002.json
```

## 文件名绑定要求

加密参数的一部分来自原始文件名，因此输出文件名必须保持游戏原始文件名。

例如：

- `Map002.json` 必须仍然叫 `Map002.json`
- `Actors.json` 必须仍然叫 `Actors.json`

不要先把解密后的文件改名成 `Map002_edit.json` 之类再加密回去。因为文件名本身参与密钥派生，改名后输出就会失效。

## 参数错误时的典型症状

如果你使用了错误参数，或者把 `--game` 指向了错误的游戏根目录，运行时最常见的症状之一是：

- `fail load MapXXX.json`

这通常说明该文件是按错误参数重新加密的，或者在错误流程下被当成明文/密文读取。

## 命令摘要

```bash
python3 rpgdata_crypt.py decrypt <encrypted_dir> <output_dir> [--pretty] [--game /path/to/game]
python3 rpgdata_crypt.py encrypt <plain_dir> <output_dir> [--game /path/to/game]
python3 rpgdata_crypt.py decrypt-file <input.json> <output.json> [--pretty] [--game /path/to/game]
python3 rpgdata_crypt.py encrypt-file <input.json> <output.json> [--game /path/to/game]
python3 rpgdata_crypt.py restore /path/to/game
python3 rpgdata_crypt.py revert /path/to/game
python3 rpgdata_crypt.py patch-js /path/to/game
```

## Rust 构建产物名称

Rust CLI crate 位于 `rpgmz_crypt_rs/`，默认构建出的二进制名为 `rpgdata_crypt`。

滚动发布使用的产物名称：

- Linux：`rpgdata_crypt-linux-x86_64`
- Windows：`rpgdata_crypt-windows-x86_64.exe`

本地 `cargo build --release` 的默认输出仍然是：

- Linux 本地 release 二进制：`target/release/rpgdata_crypt`
- Windows 本地 release 二进制：`target/release/rpgdata_crypt.exe`

## 说明

- 如果你的操作都在游戏目录树内，自动检测通常就足够，不一定需要 `--game`。
- 如果你的输入/输出路径在游戏目录树外，传 `--game /path/to/game`。
- 原版 MV 的图片/音频解密机制，不等于这里的 JSON 数据处理流程。
- 关键点在于：游戏的管理器脚本是否会在加载时解密这种包装过的 JSON 数据。
