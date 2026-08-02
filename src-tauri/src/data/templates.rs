/// 单个提示词模板
pub struct TemplateDef {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub prompt_prefix: &'static str, // 含 {concept} 占位符
}

/// 20 个内置图标风格模板。通用收尾由 generate 命令统一追加。
pub const TEMPLATES: &[TemplateDef] = &[
    // ── 扁平简约 ──
    TemplateDef {
        id: "flat-design",
        name: "扁平化",
        description: "纯色块，无阴影无渐变，最通用的应用图标",
        category: "扁平简约",
        prompt_prefix: "A flat icon of {concept}, solid colors, bold geometric shapes, no shadows, no gradients",
    },
    TemplateDef {
        id: "outline",
        name: "线性",
        description: "细线勾勒，极简，适合工具类应用",
        category: "扁平简约",
        prompt_prefix: "A line icon of {concept}, thin uniform stroke, no fill, minimal",
    },
    TemplateDef {
        id: "duotone",
        name: "双色",
        description: "两种主色，对比鲜明",
        category: "扁平简约",
        prompt_prefix: "A duotone icon of {concept}, two contrasting colors only, bold overlapping shapes",
    },
    TemplateDef {
        id: "black-white",
        name: "黑白",
        description: "纯黑白，极致简约",
        category: "扁平简约",
        prompt_prefix: "A pure black and white icon of {concept}, bold silhouette, extreme contrast, no gray",
    },
    TemplateDef {
        id: "material",
        name: "Material",
        description: "Google Material Design 规范",
        category: "扁平简约",
        prompt_prefix: "A Material Design icon of {concept}, geometric shapes, limited palette, subtle elevation, flat colors",
    },
    // ── 3D拟物 ──
    TemplateDef {
        id: "3d-render",
        name: "3D 渲染",
        description: "立体拟物风，逼真材质",
        category: "3D拟物",
        prompt_prefix: "A 3D rendered app icon of {concept}, glossy material, soft studio lighting, subtle reflections, isometric view",
    },
    TemplateDef {
        id: "skeuomorphism",
        name: "拟物风",
        description: "逼真质感，经典 iOS 风格",
        category: "3D拟物",
        prompt_prefix: "A skeuomorphic app icon of {concept}, realistic textures, detailed shading, soft drop shadow",
    },
    TemplateDef {
        id: "metallic",
        name: "金属",
        description: "金银铜金属光泽",
        category: "3D拟物",
        prompt_prefix: "A metallic app icon of {concept}, polished metal, reflective surfaces, gold/silver gleam, luxurious",
    },
    TemplateDef {
        id: "isometric",
        name: "等轴测",
        description: "3D 等距视角",
        category: "3D拟物",
        prompt_prefix: "An isometric 3D app icon of {concept}, precise geometry, clean shading, tilted perspective",
    },
    // ── 系统风 ──
    TemplateDef {
        id: "ios-system",
        name: "iOS 风",
        description: "最新 iOS 系统图标风格",
        category: "系统风",
        prompt_prefix: "An app icon of {concept} in latest iOS style, refined details, subtle gradients, soft shadows",
    },
    TemplateDef {
        id: "macos",
        name: "macOS 风",
        description: "macOS Big Sur 圆角矩形",
        category: "系统风",
        prompt_prefix: "A macOS style app icon of {concept}, squircle shape, subtle 3D depth, clean illustration",
    },
    // ── 质感特效 ──
    TemplateDef {
        id: "gradient",
        name: "渐变",
        description: "柔和渐变色，现代感",
        category: "质感特效",
        prompt_prefix: "An app icon of {concept} with smooth vibrant gradients, soft lighting",
    },
    TemplateDef {
        id: "glassmorphism",
        name: "毛玻璃",
        description: "半透明玻璃质感",
        category: "质感特效",
        prompt_prefix: "A glassmorphism app icon of {concept}, frosted glass, translucent layers, blurred backdrop, vibrant colors",
    },
    TemplateDef {
        id: "neumorphism",
        name: "新拟态",
        description: "柔和阴影浮雕",
        category: "质感特效",
        prompt_prefix: "A neumorphic app icon of {concept}, soft extruded look, subtle inner/outer shadows, monochrome",
    },
    TemplateDef {
        id: "neon",
        name: "霓虹",
        description: "赛博朋克发光",
        category: "质感特效",
        prompt_prefix: "A neon app icon of {concept}, glowing edges, dark background, electric blue and magenta, cyberpunk",
    },
    // ── 手绘艺术 ──
    TemplateDef {
        id: "hand-drawn",
        name: "手绘风",
        description: "温暖手绘插画",
        category: "手绘艺术",
        prompt_prefix: "A hand-drawn app icon of {concept}, sketchy organic lines, warm colors, cute and friendly",
    },
    TemplateDef {
        id: "watercolor",
        name: "水彩",
        description: "柔和艺术水彩",
        category: "手绘艺术",
        prompt_prefix: "A watercolor app icon of {concept}, soft washes, bleeding edges, pastel tones, artistic",
    },
    TemplateDef {
        id: "pixel-art",
        name: "像素风",
        description: "复古游戏像素艺术",
        category: "手绘艺术",
        prompt_prefix: "A pixel art app icon of {concept}, 16-bit style, crisp pixels, limited palette, retro",
    },
    TemplateDef {
        id: "cartoon-colorful",
        name: "卡通",
        description: "活泼彩色卡通",
        category: "手绘艺术",
        prompt_prefix: "A colorful cartoon app icon of {concept}, thick outlines, cel shading, bright playful colors",
    },
    TemplateDef {
        id: "emoji",
        name: "Emoji",
        description: "圆润可爱表情风",
        category: "手绘艺术",
        prompt_prefix: "An emoji style app icon of {concept}, rounded blob shape, glossy finish, vibrant, cute",
    },
];

/// 根据 ID 查找模板
pub fn get_template(id: &str) -> Option<&'static TemplateDef> {
    TEMPLATES.iter().find(|t| t.id == id)
}
