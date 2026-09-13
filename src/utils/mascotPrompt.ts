/**
 * ip-as-logo skill 提示词骨架（GenerateView 吉祥物模式）
 * 规则来源与参数设计见 docs/plans/2026-09-14-吉祥物模式.md
 * 骨架八段式逐行移植自 https://github.com/s1dashu/ip-as-logo-skill （MIT）
 */

export interface MascotPromptParams {
  /** IP 主体，如"橘猫" */
  subject: string
  /** 定义特征（skill 限定最多一个），留空由 AI 决定 */
  feature?: string
  /** 三色模式：auto = AI 按主体语境配色 */
  colorMode: 'auto' | 'custom'
  color1?: string
  color2?: string
  bgColor?: string
  /** 补充描述 */
  extra?: string
}

/** 组装单张完整提示词；corner 是批量内唯一变量（skill：奇数张左下、偶数张右下） */
export function buildMascotPrompt(p: MascotPromptParams, corner: 'lower-left' | 'lower-right'): string {
  const subject = p.subject || 'cute character'
  const custom = p.colorMode === 'custom'
  const bgDesc = custom
    ? `solid ${p.bgColor}`
    : 'a gently muted, clearly chromatic solid color chosen to suit the character'
  const colorLine = custom
    ? `Use ${p.color1} and ${p.color2} as the two IP base colors`
    : 'Choose the two IP colors from the subject and context'
  return [
    'Create one complete full-bleed 1:1 square image.',
    `Background: fill the entire square with ${bgDesc}. Keep this background visible in every open area and in the corners not occupied by the character; the ${corner} corner must be occupied by the character.`,
    `Subject: place one extremely simplified, cute, endearing ${subject} IP character on the background, reduced to one soft rounded continuous silhouette and one defining feature${p.feature ? ` (${p.feature})` : ''}.`,
    'Complexity: use only 4-7 large basic shapes and at most two broad internal color regions. Use two simple eyes and add one tiny mouth only when it helps the expression. Remove every nonessential line, outline, anatomical detail, texture, and decoration. Keep the character readable at 32 x 32.',
    `Color behavior: use exactly three semantic colors in the complete image: exactly two IP base colors plus the background color. ${colorLine}, organize both into broad purposeful masses, and reuse them for facial marks. Keep the IP, facial marks, and background clearly separated.`,
    `Composition: keep the character upright and emerging from the ${corner} corner, filling about 85-95% of the square so it remains visually dominant. Cropping at the bottom or assigned side is welcome when it strengthens the corner emergence. Preserve both paired identifying features. Never center or bottom-center the character.`,
    'Style: make simplification, cuteness, and lovable baby-like appeal the strongest qualities. Use large soft forms, compact proportions, thick rounded contours, and an ultra-clean graphic treatment. Prefer one clear shape over several explanatory details. Add an extremely, extremely subtle, almost imperceptible sense of depth through a barely-there neo-skeuomorphic treatment.',
    'Finish: show only the character on the full-canvas background, with clean surfaces and normal square outer corners.',
    ...(p.extra ? [`Additional notes: ${p.extra}.`] : []),
    'Constraints: Use no text or watermark. Add no borders, frames, cards, or presentation masks. Include one character only, with no extra subjects or scenery. Use no fragile lines, sharp tips, unnecessary outlines, tiny details, or decorative marks. Add no photorealistic material, dramatic bevel, glossy hotspot, deep occlusion, extrusion, strong three-dimensional rendering, or external cast shadow. Keep the background solid and uniform, with no texture, vignette, or lighting variation.',
  ].join('\n')
}
