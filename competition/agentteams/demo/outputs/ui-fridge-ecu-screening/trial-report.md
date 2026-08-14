# 车载冷藏设备 ECU 环境测试报告（模拟演示）
本报告使用虚构 A2L、模拟 CAN/MDA 数据和脱敏试验文档生成，不代表真实企业测试结果。
## 1. 测试目的
验证温度冲击、高温高湿/结露等环境条件下的 ECU 信号采集、统计和报告追溯流程。
## 2. 测试环境
低温 -20 °C；高温 60 °C；湿度 95 %RH。
数据链路：模拟 A2L → MDA 回放适配器 → 原始 Excel → 合规模型 → 正式报告。
## 3. A2L 文件预筛选
扫描 2 个文件，接受 1 个，拒绝 1 个。
拒绝文件不进入测量链路，并保留原因供人工复核。
## 4. 原始数据摘要
覆盖 72 个信号、6 个模拟 ECU，原始数据见 raw-data.xlsx。
统计字段：最小值、最大值、平均值、样本数。
## 5. 原始数据合规判定
按试验大纲的 6 条判定标准逐项核验：pass。
TEMP-BOX-MIN：pass；BOX_TEMP_C.min 观测范围 -18.7 ~ -16.7；标准 >= -20。
TEMP-BOX-MAX：pass；BOX_TEMP_C.max 观测范围 -17.3 ~ -15.3；标准 <= 60。
COND-HUMIDITY-MAX：pass；HUMIDITY_PERCENT.max 观测范围 74.5 ~ 93.7；标准 <= 95。
COND-FLAG-RANGE：pass；CONDENSATION_FLAG.max 观测范围 0.0 ~ 0.0；标准 <= 1。
CAN-ERROR-MAX：pass；CAN_ERROR_COUNT.max 观测范围 0.0 ~ 0.0；标准 <= 0。
CAN-VOLTAGE-RANGE：pass；SUPPLY_VOLTAGE_V.min 观测范围 11.3 ~ 11.55；标准 11 ~ 13。
## 6. 合规覆盖
ENV-TEMP-SHOCK：covered；证据：raw-data.xlsx, a2l-preflight.json
ENV-CONDENSATION：covered；证据：raw-data.xlsx, a2l-preflight.json
ENV-CAN-QUALITY：covered；证据：raw-data.xlsx, a2l-preflight.json
## 7. 验证结论
自动校验结果：needs_human_review。
原始数据按试验大纲判定：pass；该结果仅针对合成输入。
若存在无法打开的 A2L 文件，最终结论必须保留人工复核状态。
## 8. 证据索引
a2l-preflight.json；raw-data.xlsx；compliance-model.json；validation-result.json。
