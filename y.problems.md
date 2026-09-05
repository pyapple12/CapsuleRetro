# 问题备忘录（y.problems.md）

> ⚠️ **参考内容声明**：本文件当前全部为 DeepTransHub 项目的参考样例（2026-09-04 精简存档，完整原版备份于 `.temp/reference-backup/y.problems.md`），仅示范问题条目的记录格式。**CapsuleRetro 记录自有问题后，删除全部参考章节**。

## 一、文件用途与条目格式（压缩版参考）

`y.problems.md` 是问题与远期改进的备忘录：**只增不删、编号递增**（解决后不删条目，在任务清单里闭环；此处仅登记"待实现的设计决策"）。条目格式：

```markdown
{N}. **{标题}**（{YYYY-MM-DD} {状态}）：

当前问题：{现状描述、为什么是问题}

未来方案：

- {方案要点}（可含设计草稿/代码块/工作量估计）
```

- **状态取值**：记录 / 讨论确认 / 定案 / 待实现
- 条目被任务清单引用（`[problems#N]`），任务完成时回改状态
- 适合记录：暂不做但要留案的设计讨论、远期重构方向、审计中"降级备案"的关注项

---

## 二、完整参考样例：条目 3（DeepTransHub 统一 Context + config 拆分）

> 以下为 DeepTransHub 一条完整问题条目的存档，示范"带设计草稿与工作量估计的深度条目"写法（该条目后来落地为 D2 任务组，即条目从备忘录流入任务清单的完整链路）。

3. **统一 Context + config 拆分**（2026-07-24 讨论确认，待实现）

   当前问题：四个工具（process/translate/download/analyze）各自独立构建 context，config 被多次读取，逻辑大量重复。

   设计方案：

   a) **config 拆分为三层**：
   - `key.json`：API 密钥（保持独立，已 gitignore）
   - `config.json`：外部信息（excel_file、save_directory）
   - `setting.json`：程序内部设定（defaults、steps、rounds、max_retries、batch_analyze_size 等）

   b) **新建 `utils/context.py` — 统一 Context 入口**：

   ```python
   @dataclass
   class UnifiedContext:
       # config.json
       excel_file: str
       save_directory: str
       # key.json
       api_key: str
       api_base_url: str
       # setting.json
       defaults: dict
       steps: list
       rounds: list
       # 工具专用
       download_config: dict = field(default_factory=dict)
       interrupt_codes: dict = field(default_factory=dict)
       # Excel（按需打开）
       workbook: Workbook | None = None
       worksheet: Worksheet | None = None
       start_row: int | None = None

   def load_context(tool: str) -> UnifiedContext:
       # 一次性读取全部 JSON 文件（key + config + setting + download_image）
       # 构建缓存，主进程只读一次，避免四工具独立读取
       # 按 tool 决定是否打开 Excel
       # 未使用的字段 = None，不影响任何逻辑
   ```

   c) **各工具迁移**：
   - `tools/process_excel.py`：删除 `_PreprocessContext` + `_load_preprocess_context()`，改用 `load_context("process")`
   - `tools/translate.py`：删除 `_TranslateContext` + `_load_translate_context()`，改用 `load_context("translate")`
   - `tools/download_image.py`：删除 `_DownloadContext` + `_load_download_context()`，改用 `load_context("download")`
   - `tools/analyze_logs.py`：删除 `_AnalyzeContext` + `_load_analyze_context()`，改用 `load_context("analyze")`

   d) **缓存策略**：
   - `main.py` 调用 `load_context()` 后传给各工具子进程（通过 CLI 参数或环境变量）
   - 各工具不再独立读取 JSON，真正实现一次加载、全局复用

   工作量：约 150-200 行新代码 + 4 个工具的各约 30 行删除。
