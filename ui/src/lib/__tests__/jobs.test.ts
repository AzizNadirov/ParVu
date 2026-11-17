import { describe, expect, it, vi } from "vitest";

const invoke = vi.fn(async (_cmd: string, _args?: Record<string, unknown>): Promise<unknown> => undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args) }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ listen: async () => () => undefined }),
}));

import { dispatchJobProgress, isCancelled, jobFraction, jobResult, startJob, type JobProgress } from "../jobs";

describe("jobs", () => {
  it("routes progress to its own job only, until it ends", async () => {
    let finish!: (v: string) => void;
    const seenA: JobProgress[] = [];
    const seenB: JobProgress[] = [];
    const a = startJob((id) => new Promise<string>((r) => ((finish = r), void id)), { onProgress: (p) => seenA.push(p) });
    const b = startJob(async () => "b", { onProgress: (p) => seenB.push(p) });
    expect(a.id).not.toBe(b.id);
    await b.result;
    await Promise.resolve();
    dispatchJobProgress({ job: a.id, rows: 10 });
    dispatchJobProgress({ job: b.id, rows: 99 }); // b has ended: dropped
    expect(seenA).toEqual([{ job: a.id, rows: 10 }]);
    expect(seenB).toEqual([]);
    finish("a");
    expect(await a.result).toBe("a");
    dispatchJobProgress({ job: a.id, rows: 20 });
    expect(seenA).toHaveLength(1);
  });

  it("cancel asks the backend to stop that job", () => {
    const j = startJob(() => new Promise<never>(() => undefined));
    j.cancel();
    expect(invoke).toHaveBeenCalledWith("cancel_job", { job: j.id });
  });

  it("a stopped job resolves to null, other errors still throw", async () => {
    const stopped = { kind: "cancelled", message: "Cancelled" };
    expect(isCancelled(stopped)).toBe(true);
    expect(await jobResult(startJob(() => Promise.reject(stopped)))).toBeNull();
    await expect(jobResult(startJob(() => Promise.reject({ kind: "query", message: "boom" })))).rejects.toMatchObject({ kind: "query" });
  });

  it("fraction done from rows read, else DuckDB's percentage", () => {
    expect(jobFraction({ rows: 25, total: 100, percent: 90 })).toBe(0.25);
    expect(jobFraction({ rows: null, total: null, percent: 40 })).toBe(0.4);
    expect(jobFraction({ rows: null, total: null, percent: null })).toBeNull();
    expect(jobFraction({ rows: 500, total: 100, percent: null })).toBe(1);
  });
});
