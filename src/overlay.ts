// プレビューに重ねる SVG の部品（切り抜き・投稿加工・部分補正の範囲で共通）。座標の計算は regions.ts。

const SVG = "http://www.w3.org/2000/svg";
/** ハンドルの大きさと、当たり判定の半径（画面の px） */
export const HANDLE_SIZE = 8;
export const HANDLE_HIT = 10;

/** class と属性を付けた SVG の要素を作る。 */
export function svgElement(name: string, className: string, attributes: Record<string, number | string> = {}): SVGElement {
  const element = document.createElementNS(SVG, name);
  element.setAttribute("class", className);
  for (const [key, value] of Object.entries(attributes)) element.setAttribute(key, String(value));
  return element;
}

/** (cx, cy) を中心にした四角いハンドル。 */
export function handleElement(cx: number, cy: number): SVGElement {
  const half = HANDLE_SIZE / 2;
  return svgElement("rect", "handle", { x: cx - half, y: cy - half, width: HANDLE_SIZE, height: HANDLE_SIZE });
}
