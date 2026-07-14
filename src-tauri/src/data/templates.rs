/// 单个提示词模板
pub struct TemplateDef {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub prompt_prefix: &'static str, // 含 {concept} 占位符
}

/// 14 个内置图标风格模板
pub const TEMPLATES: &[TemplateDef] = &[
    // ---------- 经典风格 ----------
    TemplateDef {
        id: "flat-design",
        name: "扁平化",
        description: "Material 风格，纯色块，最通用的应用图标",
        prompt_prefix: "A flat design app icon of {concept}, single object, solid color fills, no gradients, no shadows, clean geometric shapes, bold and simple silhouette, centered composition filling the frame, soft pastel background color, high contrast, readable at small sizes, vector style, crisp edges, professional app icon design",
    },
    TemplateDef {
        id: "outline",
        name: "线性图标",
        description: "细线勾勒，极简，适合工具类应用",
        prompt_prefix: "A minimalist line icon of {concept}, single object, thin uniform stroke outline, monochrome, no fill, no background details, centered composition filling the frame, plain solid background color, high contrast, readable at small sizes, vector style, crisp edges, professional UI icon design",
    },
    TemplateDef {
        id: "gradient",
        name: "渐变风格",
        description: "柔和渐变色，现代感，适合时尚/社交应用",
        prompt_prefix: "An app icon of {concept}, single object, smooth color gradients, modern vibrant gradient fills, soft lighting, rounded shapes, subtle depth, centered composition filling the frame, clean gradient background, highly polished, readable at small sizes, professional app icon design",
    },
    // ---------- 拟物/3D ----------
    TemplateDef {
        id: "3d-render",
        name: "3D 渲染",
        description: "立体拟物风，逼真材质，适合现代应用",
        prompt_prefix: "A 3D rendered app icon of {concept}, single object, glossy material, soft studio lighting, subtle reflections, smooth surfaces, isometric perspective, centered composition filling the frame, soft gradient background, highly polished, highly detailed, octane render style, professional app icon design",
    },
    TemplateDef {
        id: "skeuomorphism",
        name: "拟物风",
        description: "逼真质感，细节丰富，经典 iOS 风",
        prompt_prefix: "A skeuomorphic app icon of {concept}, single object, realistic textures and materials, detailed shading, soft drop shadow, centered composition filling the frame, textured background, highly detailed, photorealistic, classic iOS app icon style",
    },
    // ---------- 系统风格 ----------
    TemplateDef {
        id: "ios-system",
        name: "iOS 系统风",
        description: "最新 iOS 系统图标风格，精致高完成度",
        prompt_prefix: "A highly polished app icon of {concept} in the style of the latest iOS system icons, single object, refined details, subtle gradients, soft shadows with consistent light direction, centered composition filling the frame, saturated gradient background color, highly polished, professional finish, readable at small sizes, App Store quality",
    },
    TemplateDef {
        id: "macos",
        name: "macOS 风",
        description: "macOS Big Sur+ 圆角矩形风格",
        prompt_prefix: "A macOS style app icon of {concept}, single object, squircle shape, clean modern illustration, subtle 3D depth, soft gradient background, centered composition filling the frame, highly polished, refined details, Apple design guidelines, professional app icon",
    },
    TemplateDef {
        id: "material",
        name: "Material 风",
        description: "Google Material Design 规范",
        prompt_prefix: "A Material Design icon of {concept}, single object, following Google Material guidelines, consistent geometric shapes, limited color palette, flat with subtle elevation, centered composition filling the frame, solid background color, readable at small sizes, vector style, crisp edges",
    },
    // ---------- 特色风格 ----------
    TemplateDef {
        id: "glassmorphism",
        name: "毛玻璃风",
        description: "半透明玻璃质感，现代 iOS 控件风",
        prompt_prefix: "A glassmorphism app icon of {concept}, single object, frosted glass effect, translucent material, blurred backdrop, vibrant gradient showing through glass, centered composition filling the frame, modern, highly polished, professional app icon design",
    },
    TemplateDef {
        id: "duotone",
        name: "双色图标",
        description: "两种主色，对比鲜明，适合品牌图标",
        prompt_prefix: "A duotone app icon of {concept}, single object, two-tone color scheme only, overlapping shapes with multiply blend, bold contrast between the two colors, centered composition filling the frame, solid background color, readable at small sizes, vector style, professional app icon design",
    },
    TemplateDef {
        id: "neumorphism",
        name: "新拟态",
        description: "柔和阴影浮雕，UI 控件风",
        prompt_prefix: "A neumorphic app icon of {concept}, single object, soft extruded plastic look, subtle inner and outer shadows, monochrome with single accent color, centered composition filling the frame, same background color as object, soft UI design, highly polished",
    },
    TemplateDef {
        id: "pixel-art",
        name: "像素风",
        description: "复古游戏像素艺术",
        prompt_prefix: "A pixel art app icon of {concept}, single object, 16-bit retro game style, crisp pixels, limited color palette, centered composition filling the frame, solid background color, readable at small sizes, professional pixel art icon",
    },
    TemplateDef {
        id: "hand-drawn",
        name: "手绘风",
        description: "温暖手绘插画，适合儿童/生活类应用",
        prompt_prefix: "A hand-drawn app icon of {concept}, single object, cute illustration style, warm colors, organic sketchy lines, friendly and approachable, centered composition filling the frame, soft background color, highly polished, professional app icon design",
    },
    TemplateDef {
        id: "emoji",
        name: "Emoji 风",
        description: "圆润可爱表情风，适合社交应用",
        prompt_prefix: "A cute emoji-style app icon of {concept}, single object, rounded blob shape, expressive face if applicable, vibrant colors, glossy finish, friendly, centered composition filling the frame, solid bright background color, highly polished, professional app icon design",
    },
];

/// 根据 ID 查找模板
pub fn get_template(id: &str) -> Option<&'static TemplateDef> {
    TEMPLATES.iter().find(|t| t.id == id)
}
