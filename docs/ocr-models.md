# OCR 模型下载与镜像

Demiurge 的 OCR 使用本地 PP-OCRv5 mobile 模型。模型不会随安装包分发，需要用户在 Settings > Tools > Computer Use / OCR 中下载，或手动放入应用数据目录。

## 模型源

- **ModelScope**：推荐中国大陆网络环境使用。Settings 中的默认源会从 `https://modelscope.cn/models/greatv/oar-ocr` 下载。
- **Hugging Face**：适合能够稳定访问 Hugging Face 的网络环境，源地址为 `https://huggingface.co/monkt/paddleocr-onnx`。

两个源最终都会落到同一组本地文件名，运行时只看本地文件是否存在。

## 必需文件

模型目录由应用数据目录决定，可在 Settings 的 OCR 状态卡片中查看。目录下必须有：

```text
pp-ocrv5_mobile_det.onnx
pp-ocrv5_mobile_rec.onnx
ppocrv5_dict.txt
```

如果内置下载失败，可以从 Settings 中当前源的文件链接手动下载缺失文件，并把它们放到同一个模型目录。文件名必须保持上面的目标名称。

## 失败排查

- 如果状态显示 `Missing`，先确认三个文件都在状态卡片显示的模型目录下，且文件大小大于 0。
- 如果下载卡在某个文件，切换 ModelScope / Hugging Face 后重试；中国大陆网络优先 ModelScope。
- 如果模型已经手动放好，点击 Settings 里的 `Refresh` 重新读取状态。
- 如果 OCR 工具仍报模型缺失，重启应用会清理旧的 OCR engine 缓存并重新检查本地文件。
