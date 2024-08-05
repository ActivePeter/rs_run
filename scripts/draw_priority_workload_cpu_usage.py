import numpy as np
import matplotlib.pyplot as plt

# time mode
data={"0,0":0.014926461,"0,1":0.015238124,"0,2":0.16714798,"0,3":0.035317596,"0,4":0.028813925,"1,0":0.0150074605,"1,1":0.015448302,"1,2":0.19455698,"1,3":0.06179792,"1,4":0.020044457,"2,0":0.015121032,"2,1":0.015714716,"2,2":0.21804985,"2,3":0.08033019,"2,4":0.017892463,"3,0":0.015127492,"3,1":0.015721576,"3,2":0.20886259,"3,3":0.07611441,"3,4":0.02387363,"4,0":0.015136925,"4,1":0.015740223,"4,2":0.21275845,"4,3":0.066312894,"4,4":0.017968092}
# count mode
# data = {"0,0":0.015897611,"0,1":0.015748644,"0,2":0.20483066,"0,3":0.02926367,"0,4":0.04280346,"1,0":0.01600011,"1,1":0.015752306,"1,2":0.22690775,"1,3":0.047639567,"1,4":0.017053176,"2,0":0.015279182,"2,1":0.015745252,"2,2":0.23216243,"2,3":0.08652877,"2,4":0.020415194,"3,0":0.015839372,"3,1":0.01575208,"3,2":0.2256634,"3,3":0.07134362,"3,4":0.023797475,"4,0":0.015160647,"4,1":0.01576439,"4,2":0.22170347,"4,3":0.08103041,"4,4":0.02458655}

# 解构数据
priorities = [0, 1, 2, 3, 4]
load_types = [0, 1, 2, 3, 4]
values = np.zeros((len(priorities), len(load_types)))

for key, value in data.items():
    p, l = map(int, key.split(','))
    values[p, l] = value

# 绘图配置
bar_width = 0.15
index = np.arange(len(load_types))

# 创建柱状图
fig, ax = plt.subplots()
bars = []
colors = ['b', 'g', 'r', 'c', 'm']

for i in range(len(priorities)):
    bar = ax.bar(index + i * bar_width, values[i], bar_width, label=f'Priority {i}', color=colors[i])
    bars.append(bar)

# 添加值标签
def add_labels(rects):
    for rect in rects:
        height = rect.get_height()
        ax.annotate('{}'.format(height),
                    xy=(rect.get_x() + rect.get_width() / 2, height),
                    xytext=(0, 3),  # 3 points vertical offset
                    textcoords="offset points",
                    rotation=90,
                    ha='center', va='bottom')

for bar_group in bars:
    add_labels(bar_group)

# 设置图表标题和坐标轴标签
ax.set_title('CPU Usage by Priority and Load Type')
ax.set_xlabel('Workload Type')
ax.set_ylabel('CPU Usage')
ax.set_xticks(index + bar_width * 2)
ax.set_xticklabels(load_types)
ax.legend()

# 显示图表
plt.show()