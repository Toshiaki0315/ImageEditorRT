// 遠近の補正の向きを、回転・反転に合わせて直す（画面の部品に依存しない）。
// 縦: 正で上の辺が細くなった台形を直す。横: 正で左の辺が細くなった台形を直す。

import type { OrientOp } from "./types";

/** 回転・反転したあとの遠近の補正（同じ写真の辺を直し続けるよう、縦横・符号を入れ替える）。 */
export function orientPerspective(op: OrientOp, vertical: number, horizontal: number): [number, number] {
  const flip = (v: number) => (v === 0 ? 0 : -v);
  switch (op) {
    // 右に回すと、上の辺は右へ、左の辺は上へ
    case "rotate_right":
      return [horizontal, flip(vertical)];
    // 左に回すと、上の辺は左へ、右の辺は上へ
    case "rotate_left":
      return [flip(horizontal), vertical];
    case "flip_horizontal":
      return [vertical, flip(horizontal)];
    case "flip_vertical":
      return [flip(vertical), horizontal];
  }
}
