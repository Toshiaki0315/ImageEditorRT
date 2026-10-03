// アンドゥ／リドゥの履歴（旧版 FR-UI-43）。画面の部品には依存しない（tests-ts/history.test.ts で確かめる）。

/** 履歴の最大の件数（古いものから消す） */
export const HISTORY_LIMIT = 100;
/** 続けて変えた分を 1 回の操作にまとめるため、変更が落ち着くまで待つ時間 (ms) */
export const HISTORY_DELAY_MS = 500;

/** JSON にした文字列（オブジェクトのキーの並びによらず同じになるよう、キーを並べ替える）。 */
const stableJson = (value: unknown) =>
  JSON.stringify(value, (_key, v: unknown) =>
    v && typeof v === "object" && !Array.isArray(v)
      ? Object.fromEntries(Object.entries(v).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)))
      : v,
  );

/** 値として等しいか（設定はどれも JSON にできる値。キーの並びは問わない）。 */
export const sameValue = (a: unknown, b: unknown) => stableJson(a) === stableJson(b);

/**
 * 状態のスナップショットを積む履歴。current() が今の状態で、undo() で 1 つ前、redo() で 1 つ先に移る。
 * 新しい状態を push() すると、やり直せる先は捨てる。古い状態は limit 件まで残す。
 */
export class History<T> {
  private readonly undos: T[] = [];
  private readonly redos: T[] = [];
  private present: T;
  private readonly limit: number;

  constructor(initial: T, limit = HISTORY_LIMIT) {
    if (limit < 1) throw new RangeError(`limit は 1 以上で指定してください: ${limit}`);
    this.present = initial;
    this.limit = limit;
  }

  current(): T {
    return this.present;
  }

  /** 新しい状態を積む。今の状態と同じなら何もせず false を返す。 */
  push(state: T): boolean {
    if (sameValue(state, this.present)) return false;
    this.undos.push(this.present);
    if (this.undos.length > this.limit) this.undos.shift();
    this.present = state;
    this.redos.length = 0;
    return true;
  }

  canUndo(): boolean {
    return this.undos.length > 0;
  }

  canRedo(): boolean {
    return this.redos.length > 0;
  }

  /** 1 つ前の状態に戻って返す。戻せなければ今の状態を返す。 */
  undo(): T {
    const previous = this.undos.pop();
    if (previous !== undefined) {
      this.redos.push(this.present);
      this.present = previous;
    }
    return this.present;
  }

  /** 1 つ先の状態に進んで返す。進めなければ今の状態を返す。 */
  redo(): T {
    const next = this.redos.pop();
    if (next !== undefined) {
      this.undos.push(this.present);
      this.present = next;
    }
    return this.present;
  }

  /** 履歴を消して、initial を今の状態にする。 */
  reset(initial: T) {
    this.undos.length = 0;
    this.redos.length = 0;
    this.present = initial;
  }
}

/** 時間待ちの関数（テストでは差し替える）。 */
export type Timers = {
  set: (callback: () => void, ms: number) => unknown;
  clear: (handle: unknown) => void;
};

const realTimers: Timers = {
  set: (callback, ms) => setTimeout(callback, ms),
  clear: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

/**
 * 設定の変更を履歴に積む係。変更は落ち着いてから（スライダー・範囲のドラッグ中は離すまで）1 回の操作として積む。
 * まだ積んでいない変更も、元に戻す・やり直すの前に積むので、すぐに戻せる。
 */
export type RecorderOptions<T> = {
  /** 今の状態を取る */
  snapshot: () => T;
  /** 状態を画面と設定に戻す */
  restore: (state: T) => void;
  /** スライダー・範囲をドラッグしている間は true（積むのを待つ） */
  isAdjusting: () => boolean;
  /** 戻せる・やり直せるが変わったかもしれないとき（メニューの状態を合わせる） */
  onUpdate: () => void;
  delay?: number;
  limit?: number;
  timers?: Timers;
};

export class HistoryRecorder<T> {
  private readonly history: History<T>;
  private readonly options: RecorderOptions<T>;
  private timer: unknown = null;
  /** 履歴の状態を画面に戻している間（その間の変更は積まない） */
  private restoring = false;

  constructor(options: RecorderOptions<T>) {
    this.options = options;
    this.history = new History(options.snapshot(), options.limit ?? HISTORY_LIMIT);
  }

  private get timers(): Timers {
    return this.options.timers ?? realTimers;
  }

  /** 積んでいない変更があるか。 */
  get pending(): boolean {
    return this.timer !== null;
  }

  /** 戻せるか（積んでいない変更があれば、それを戻せる）。 */
  canUndo(): boolean {
    return this.history.canUndo() || this.pending;
  }

  /** やり直せるか（積んでいない変更があれば、やり直せる先は消えるので false）。 */
  canRedo(): boolean {
    return this.history.canRedo() && !this.pending;
  }

  /** 設定を変えたとき。落ち着いてから積む。 */
  changed() {
    if (this.restoring) return;
    this.schedule();
    this.options.onUpdate();
  }

  /** 今の状態を積む。ドラッグ中は離すまで待つ（force なら待たない）。 */
  commit(force = false) {
    this.cancel();
    if (this.options.isAdjusting() && !force) {
      this.schedule();
      return;
    }
    this.history.push(this.options.snapshot());
    this.options.onUpdate();
  }

  /** 1 つ元に戻す。戻したら true。 */
  undo(): boolean {
    this.commit(true);
    if (!this.history.canUndo()) return false;
    this.apply(this.history.undo());
    return true;
  }

  /** 1 つやり直す。やり直したら true。 */
  redo(): boolean {
    this.commit(true);
    if (!this.history.canRedo()) return false;
    this.apply(this.history.redo());
    return true;
  }

  /** 履歴を消して、今の状態を始まりにする（画像の読み込み・リセット）。 */
  reset() {
    this.cancel();
    this.history.reset(this.options.snapshot());
    this.options.onUpdate();
  }

  private apply(state: T) {
    this.restoring = true;
    try {
      this.options.restore(state);
    } finally {
      this.restoring = false;
    }
    this.cancel();
    this.options.onUpdate();
  }

  private schedule() {
    this.cancel();
    this.timer = this.timers.set(() => {
      this.timer = null;
      this.commit();
    }, this.options.delay ?? HISTORY_DELAY_MS);
  }

  private cancel() {
    if (this.timer !== null) this.timers.clear(this.timer);
    this.timer = null;
  }
}
