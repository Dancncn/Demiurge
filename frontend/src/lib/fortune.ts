// 每日吉签 · 情感陪伴功能
//
// 定位：每天可抽一支签，签文偏吉、温暖、让人看着开心；mild（末吉）占比极低，
// 即便抽到也是"先小不顺、结尾转吉"的劝勉型，绝不凶险。
//
// 存储：与 demiurge.lang 同样使用 localStorage（key: demiurge.fortune），只保留最近一次
// 抽签记录。每日一次的限制即"记录中的 date 是否等于今天"。

export type FortuneLevel = "supreme" | "great" | "good" | "fair" | "mild";

export interface FortuneEntry {
  /** 签号，如 career-01 */
  id: string;
  /** 等级 */
  level: FortuneLevel;
  /** 签题：4-7 字吉祥话或诗意短语 */
  title: string;
  /** 签诗：两句对仗，用 ／ 分隔 */
  verse: string;
  /** 解签：1-2 句温暖解释，贴合现代生活 */
  interpretation: string;
  /** 祝福语：一句直接对用户的祝福 */
  blessing: string;
}

export interface FortuneRecord {
  /** 本地日期 YYYY-MM-DD */
  date: string;
  /** 抽中的签号 */
  entryId: string;
  /** 抽取时间戳 ms */
  drawnAt: number;
  /** 记录 schema 版本，用于未来字段演进时的迁移判定 */
  version: number;
}

/** 当前 FortuneRecord schema 版本；读取时若不匹配走迁移或视为 null（允许重抽）。 */
const CURRENT_VERSION = 1;

const LS_KEY = "demiurge.fortune";
const LS_DISMISSED_KEY = "demiurge.fortune.dismissed";
const LS_AUTO_PROMPT_KEY = "demiurge.fortune.auto_prompt";

// 权重：偏吉。supreme+great+good 合计 72%，fair 22%，mild 仅 6%。
// 符合「少点凶签」的诉求——即便 mild 也是劝勉转吉，不会真的凶。
const LEVEL_WEIGHTS: Record<FortuneLevel, number> = {
  supreme: 16,
  great: 26,
  good: 30,
  fair: 22,
  mild: 6,
};

// 等级展示元数据：标签底色 / 强调色 / 标题色。配色偏中式暖调，level 越高越红金。
export const LEVEL_META: Record<
  FortuneLevel,
  { label: string; chipBg: string; chipText: string; accent: string; ring: string }
> = {
  supreme: { label: "上上签", chipBg: "#fff1e6", chipText: "#9a3412", accent: "#b91c1c", ring: "#f0c674" },
  great: { label: "上吉", chipBg: "#ffedd5", chipText: "#9a3412", accent: "#c2410c", ring: "#fdba74" },
  good: { label: "中吉", chipBg: "#dcfce7", chipText: "#166534", accent: "#0f766e", ring: "#86efac" },
  fair: { label: "中平", chipBg: "#eef1f5", chipText: "#475569", accent: "#475569", ring: "#cbd5e1" },
  mild: { label: "末吉", chipBg: "#f5efe6", chipText: "#7c5e3a", accent: "#8d6e4e", ring: "#d8c4a0" },
};

// 签文库（36 条，五主题：事业学业 / 感情人际 / 健康平安 / 综合时运 / 末吉劝勉）。
// 权重分布：supreme 6 · great 9 · good 12 · fair 5 · mild 4。mild 全部为"转吉"收尾。
export const FORTUNES: FortuneEntry[] = [
  // —— 事业·学业·前程 ——
  {
    id: "career-01",
    level: "supreme",
    title: "紫气东来",
    verse: "积薪三载无人问／一炬冲天动九衢",
    interpretation: "长期积攒的本事终于到了被看见的时候，事业学业正迎来漂亮的上升期，放手去做那件你想了很久的事。",
    blessing: "愿你付出皆有回响，前程似锦绣铺就。",
  },
  {
    id: "career-02",
    level: "supreme",
    title: "一鹤冲天",
    verse: "长风万里送秋雁／可以就此接青云",
    interpretation: "你正站在一个关键节点上，天时地利人和都凑齐了，抓住这次机会，它会成为日后回想起来都骄傲的转折。",
    blessing: "愿你今日心之所向，皆能化为手中所握。",
  },
  {
    id: "career-03",
    level: "great",
    title: "云开月明",
    verse: "拨云终见一轮月／照得前路步步明",
    interpretation: "手头正在推进的事会有看得见的进展，思路逐渐清晰，稳步往前走就好，不必焦虑节奏。",
    blessing: "愿你眼里有光，脚下有路，前途坦荡。",
  },
  {
    id: "career-04",
    level: "great",
    title: "步步生莲",
    verse: "莫道山行迟且缓／春来步步有花开",
    interpretation: "前段时间的沉淀开始起作用了，能力和机会正在慢慢对上，继续做你认为对的事，量变会悄然走向质变。",
    blessing: "愿你不疾不徐，把每一步都走成风景。",
  },
  {
    id: "career-05",
    level: "great",
    title: "顺风扬帆",
    verse: "潮平两岸阔无际／风正一帆悬正好",
    interpretation: "近期会有人愿意推你一把、给你机会或引荐资源，保持开放，别怕开口，真诚的人会遇见真诚的回应。",
    blessing: "愿你前路有知己同行，行而不孤。",
  },
  {
    id: "career-06",
    level: "good",
    title: "静水流深",
    verse: "细水长流终到海／闲花落地亦成春",
    interpretation: "不算惊艳，但踏实向好，今天的努力像浇水，没立刻开花但根在扎深，别拿一时结果否定方向。",
    blessing: "愿你今日所做之事，都在悄悄长成你想要的样子。",
  },
  {
    id: "career-07",
    level: "good",
    title: "厚积薄发",
    verse: "涓滴不弃终成海／微光渐聚自满天",
    interpretation: "手头那些细碎、不起眼的事其实都在悄悄铺路，哪怕每天只前进一点点，累积起来会超出你的预期。",
    blessing: "愿你日积寸功，终成一篑之功。",
  },
  {
    id: "career-08",
    level: "good",
    title: "守正出奇",
    verse: "莫嫌此际行步缓／自有长风送客归",
    interpretation: "当下或许进展没那么快，但稳本身就是一种福气，先把手头的事做扎实，风很快就会朝你吹来。",
    blessing: "愿你踏实走稳每一步，山高不阻有心人。",
  },

  // —— 感情·人际·姻缘 ——
  {
    id: "relation-01",
    level: "supreme",
    title: "良缘天定",
    verse: "红线暗牵三生石／清风徐引并蒂莲",
    interpretation: "缘分这事，往往在你不再强求时悄悄到来。该来的人，其实已经在路上了。",
    blessing: "愿你心里的人，也正把你放在心上。",
  },
  {
    id: "relation-02",
    level: "supreme",
    title: "花好月圆",
    verse: "灯前笑语盈盈暖／月下同心步步明",
    interpretation: "有人愿意陪你走过平淡日子，这份相守本身就是最大的福气，值得好好珍惜。",
    blessing: "愿你身边总有懂你的人，夜话有人听，天冷有人暖。",
  },
  {
    id: "relation-03",
    level: "great",
    title: "知己相逢",
    verse: "一杯淡酒逢知己／两段清欢话此生",
    interpretation: "真正聊得来的人不必多，一两个就够暖很多年。最近会有让你卸下防备的相处。",
    blessing: "愿你不必假装，也有人喜欢真实的你。",
  },
  {
    id: "relation-04",
    level: "great",
    title: "春风化雨",
    verse: "春风不语花先觉／细雨无声叶自舒",
    interpretation: "有些关系正慢慢回暖，不必急着追问结果，温柔以待，自然会更亲。",
    blessing: "愿你付出的善意，都会在合适的时候回到身边。",
  },
  {
    id: "relation-05",
    level: "great",
    title: "和合如初",
    verse: "旧雨重逢情更暖／初心不改意愈深",
    interpretation: "曾走远的人或许正找着回来的理由，留一扇门，缘分自会来敲门。",
    blessing: "愿你珍惜的人，也始终把你放在心上。",
  },
  {
    id: "relation-06",
    level: "good",
    title: "晴窗对坐",
    verse: "晴窗对坐茶初沸／笑眼相看意正浓",
    interpretation: "平淡日子里能有一个安心相处的人，就是难得的好时光，不必轰轰烈烈。",
    blessing: "愿你今日的相处，轻松自在没有负担。",
  },
  {
    id: "relation-07",
    level: "good",
    title: "春水初生",
    verse: "春水初生痕渐暖／新枝半吐意微醺",
    interpretation: "一段新的缘分正在悄悄萌芽，不必催它，让它自然生长就好。",
    blessing: "愿你遇见的每一段缘分，都值得期待。",
  },
  {
    id: "relation-08",
    level: "good",
    title: "晚风归信",
    verse: "晚风送暖归人有信／旧燕回巢好事相迎",
    interpretation: "等的人终会有消息，迟一点没关系，该回来的不会走散。",
    blessing: "愿你等的回音，比想象中更温柔地到来。",
  },

  // —— 健康·平安·出行·日常 ——
  {
    id: "health-01",
    level: "good",
    title: "身安步稳",
    verse: "清晨风过松枝暖／午后阳光满袖香",
    interpretation: "身体舒展、精神饱满的日子，做什么都顺手，连风都在帮你。",
    blessing: "愿你今日身轻步稳，所行之处皆是坦途。",
  },
  {
    id: "health-02",
    level: "fair",
    title: "云淡风轻",
    verse: "一壶温水半窗光／闲听檐雨落心房",
    interpretation: "今天没有大喜大悲，平淡里藏着最稳的福气，慢慢走就好。",
    blessing: "愿你心安如水，纵有风浪也从容以对。",
  },
  {
    id: "health-03",
    level: "good",
    title: "行路逢春",
    verse: "鞋底轻沾三月露／行囊微带远方香",
    interpretation: "出门顺利，路途安稳，连陌生的转角都藏着善意。",
    blessing: "愿你一路有清风相伴，去见想见的风景。",
  },
  {
    id: "health-04",
    level: "fair",
    title: "夜半灯暖",
    verse: "窗外月华微半掩／枕边好梦正初成",
    interpretation: "夜里翻来覆去也无妨，闭眼前给自己一个微笑，明天自有温柔在等。",
    blessing: "愿你今夜好眠，明日醒来仍觉人间值得。",
  },
  {
    id: "health-05",
    level: "good",
    title: "食安梦甜",
    verse: "三餐烟火皆有味／一夜清眠到天明",
    interpretation: "好好吃饭、好好走路，今天最该被夸的，就是那个没亏待自己的你。",
    blessing: "愿你按时吃饭、按时休息，把这副身体照顾得妥妥帖帖。",
  },
  {
    id: "health-06",
    level: "fair",
    title: "平安无事",
    verse: "门内灯火长不灭／阶前苔痕日渐深",
    interpretation: "今天没什么特别的事，但平平安安本身就是难得的好日子，别小看它。",
    blessing: "愿你今日行止安泰，平安是福，平淡亦是。",
  },
  {
    id: "health-07",
    level: "good",
    title: "晨光映窗",
    verse: "晨起推窗迎薄雾／一身轻健步生风",
    interpretation: "活动一下筋骨，整个人就亮堂起来，身体比你想的更愿意陪着你向前。",
    blessing: "愿你筋骨舒活，心中有光，步履所及皆是好风景。",
  },
  {
    id: "health-08",
    level: "fair",
    title: "步缓心宽",
    verse: "慢行半步留余力／稳踏长街不慌忙",
    interpretation: "出门赶路难免急，慢半拍反而更顺，稳稳的才走得远。",
    blessing: "愿你今日从容出门，平安归来，灯下有人等你。",
  },

  // —— 综合·时运·心境 ——
  {
    id: "fortune-01",
    level: "supreme",
    title: "紫气东来",
    verse: "东风解意送春暖／紫气盈门照福长",
    interpretation: "近来你的气场特别亮，像被一阵好风稳稳托着，顺心的事会一件接一件到来。",
    blessing: "愿你这一阵顺遂得像被风托着，心想的事一件件落地。",
  },
  {
    id: "fortune-02",
    level: "supreme",
    title: "云开月明",
    verse: "云收雾敛千山霁／月照长川万里明",
    interpretation: "走过一段闷的日子，天要放晴了，你心里那点盼头终于要迎来回应。",
    blessing: "愿你抬头就有光，低头就有暖，日子越过越舒展。",
  },
  {
    id: "fortune-03",
    level: "great",
    title: "春风及第",
    verse: "三春花发千枝秀／十里风送一帆轻",
    interpretation: "最近的努力正在悄悄发芽，不必急，它们会以你意想不到的方式回赠你。",
    blessing: "愿你被生活悄悄偏爱，惊喜总在转角等你。",
  },
  {
    id: "fortune-04",
    level: "great",
    title: "玉成其事",
    verse: "细雨润田苗渐秀／勤人得果岁方丰",
    interpretation: "手头的事渐渐有了起色，像攒够了柴火终于把饭蒸熟，踏实又满足。",
    blessing: "愿你心里有底，手上有活，日子越过越有滋味。",
  },
  {
    id: "fortune-05",
    level: "great",
    title: "晴窗煮茶",
    verse: "窗含新霁日初暖／盏浮清香气正长",
    interpretation: "你会迎来一段清清爽爽的日子，心里那些堵着的东西，会一点点散开。",
    blessing: "愿你往后的日子，晴多雨少，想见的都能见着。",
  },
  {
    id: "fortune-06",
    level: "good",
    title: "花开富贵",
    verse: "庭前细柳抽新绿／枝上初花报好音",
    interpretation: "生活里那些细微的好正在向你聚拢，像花一片片开起来，虽不轰烈，却很温柔。",
    blessing: "愿你被一件件小小的开心填满，回头一看都是暖。",
  },
  {
    id: "fortune-07",
    level: "good",
    title: "静水流深",
    verse: "闲云不动天心稳／流水无声润物长",
    interpretation: "今天没什么大事，却也平平稳稳，这种不吵不闹的踏实，本身就是难得的好福气。",
    blessing: "愿你今天不慌不忙，把日子过得稳稳当当、有滋有味。",
  },
  {
    id: "fortune-08",
    level: "fair",
    title: "守静待时",
    verse: "暂收锋芒藏静气／且待风起再扬帆",
    interpretation: "眼下或许有点闷、有点慢，别急——这段缓冲正为接下来的顺做准备，过几天就通透了。",
    blessing: "愿你耐得住这一小段，后面接住的是一整个亮堂堂的好天气。",
  },

  // —— 末吉·劝勉型（少量，温和不凶，结尾转吉） ——
  {
    id: "mild-01",
    level: "mild",
    title: "云开月明",
    verse: "小坐且听檐外雨／徐行自有月来迎",
    interpretation: "眼下或许有点闷、有点卡，像等一趟迟来的雨；但别急，属于你的那阵清风已经在路上了。",
    blessing: "愿你熬过小小的卡顿，迎面就是清朗的好消息。",
  },
  {
    id: "mild-02",
    level: "mild",
    title: "迟暖花开",
    verse: "春迟未肯轻寒去／花慢终将锦绣回",
    interpretation: "手头的事慢一点、笨一点没关系，慢慢走的路反而走得稳，日子正悄悄向好里转。",
    blessing: "愿你那些慢下来的时光，终会酿成最好的答案。",
  },
  {
    id: "mild-03",
    level: "mild",
    title: "柳暗花明",
    verse: "山路微弯疑无径／柳梢忽见有人家",
    interpretation: "这几日难免遇到些磕绊，不必较真，退半步、笑一笑，转机就藏在你不执着的那个瞬间。",
    blessing: "愿你心宽处，事情也跟着顺起来。",
  },
  {
    id: "mild-04",
    level: "mild",
    title: "雨过天晴",
    verse: "昨夜风急敲竹户／今朝晴日满窗台",
    interpretation: "看似不顺利的一阵子，其实是老天在帮你筛掉不合适的，留下来的会格外值得。",
    blessing: "愿你今日所有的小麻烦，最后都化作意料之外的欢喜。",
  },
];

/** 本地日期 YYYY-MM-DD（不用 toISOString，避免跨日时区错位）。 */
function todayStr(): string {
  const d = new Date();
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/** 按权重随机一个等级。 */
function weightedLevel(): FortuneLevel {
  const total = Object.values(LEVEL_WEIGHTS).reduce((a, b) => a + b, 0);
  let r = Math.random() * total;
  for (const lvl of Object.keys(LEVEL_WEIGHTS) as FortuneLevel[]) {
    r -= LEVEL_WEIGHTS[lvl];
    if (r <= 0) return lvl;
  }
  return "good";
}

/** 读取今日抽签记录；若今天尚未抽过返回 null。 */
export function getTodayRecord(): FortuneRecord | null {
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (!raw) return null;
    const rec = JSON.parse(raw) as Partial<FortuneRecord>;
    if (!rec || typeof rec.date !== "string" || typeof rec.entryId !== "string") return null;
    // 版本不匹配（旧版/损坏记录）：视为可重抽，避免用户被坏记录锁死永远抽不到。
    if (rec.version !== CURRENT_VERSION) return null;
    return rec.date === todayStr() ? (rec as FortuneRecord) : null;
  } catch {
    return null;
  }
}

/** 今天是否还能抽签。 */
export function canDrawToday(): boolean {
  return getTodayRecord() === null;
}

/** 按 id 查找签文。 */
export function findEntry(id: string): FortuneEntry | undefined {
  return FORTUNES.find((f) => f.id === id);
}

/**
 * 抽一支签：按等级权重选中等级，再从该等级池中均匀随机一条，
 * 写入 localStorage 作为今日记录，返回签文。
 *
 * 若该等级池意外为空（数据未加载），回退到全库随机一条，保证永远抽得到。
 */
export function drawFortune(): FortuneEntry {
  const level = weightedLevel();
  const pool = FORTUNES.filter((f) => f.level === level);
  const entry =
    pool.length > 0
      ? pool[Math.floor(Math.random() * pool.length)]
      : FORTUNES[Math.floor(Math.random() * FORTUNES.length)];

  const record: FortuneRecord = {
    date: todayStr(),
    entryId: entry.id,
    drawnAt: Date.now(),
    version: CURRENT_VERSION,
  };
  try {
    localStorage.setItem(LS_KEY, JSON.stringify(record));
  } catch {
    /* localStorage 不可用时跳过，功能仍可使用 */
  }
  return entry;
}

/** 清除今日抽签记录（版本不匹配/数据损坏时让用户当日可重抽）。 */
export function resetTodayRecord(): void {
  try {
    localStorage.removeItem(LS_KEY);
  } catch {
    /* ignore */
  }
}

// —— 每日引导弹窗的打扰控制 ——
// 「今日已忽略」：用户在 guide 态（未抽就关）关闭弹窗时标记，当日不再自动弹。
// 「自动弹窗开关」：用户可在设置里完全关闭启动自动弹窗，仅保留 Dashboard 入口卡片。

/** 标记今日已忽略引导弹窗（未抽就关）。 */
export function markDismissedToday(): void {
  try {
    localStorage.setItem(LS_DISMISSED_KEY, todayStr());
  } catch {
    /* ignore */
  }
}

/** 今日是否已忽略引导弹窗。 */
export function isDismissedToday(): boolean {
  try {
    return localStorage.getItem(LS_DISMISSED_KEY) === todayStr();
  } catch {
    return false;
  }
}

/** 自动弹窗开关是否启用（默认 true；用户在设置里关闭后启动不再自动弹）。 */
export function isAutoPromptEnabled(): boolean {
  try {
    const v = localStorage.getItem(LS_AUTO_PROMPT_KEY);
    // 未设置视为启用；显式 "0" 视为关闭。
    return v !== "0";
  } catch {
    return true;
  }
}

/** 设置自动弹窗开关。 */
export function setAutoPromptEnabled(enabled: boolean): void {
  try {
    localStorage.setItem(LS_AUTO_PROMPT_KEY, enabled ? "1" : "0");
  } catch {
    /* ignore */
  }
}
