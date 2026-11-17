// Long backend work (unique values, statistics, charts, search, counts) runs
// as a job: the backend works on its own connection, so the grid stays
// usable, sends `job-progress` events, and stops on cancel. This module
// starts jobs, routes their progress, and keeps a list of the running ones
// for the status bar.
import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export interface JobProgress {
  job: number;
  /** DuckDB's own estimate, 0–100, when it has one. */
  percent?: number;
  /** Rows read so far, when the work counts them. */
  rows?: number;
  /** Partial results (e.g. unique values found so far). */
  data?: unknown;
  /** The work starts over (a CSV column was re-read as text): drop partial results. */
  restart?: boolean;
}

export interface Job<T> {
  id: number;
  result: Promise<T>;
  cancel(): void;
}

/** A running job as the status bar shows it. */
export interface RunningJob {
  id: number;
  label: string;
  percent: number | null;
  rows: number | null;
  total: number | null;
  cancel(): void;
}

export const isCancelled = (e: unknown) => (e as { kind?: string } | null)?.kind === "cancelled";

const handlers = new Map<number, (p: JobProgress) => void>();
let listening: Promise<unknown> | null = null;

function listenOnce(): Promise<unknown> {
  if (!listening) {
    try {
      listening = getCurrentWindow()
        .listen<JobProgress>("job-progress", (e) => handlers.get(e.payload.job)?.(e.payload))
        .catch(() => null);
    } catch {
      // Outside Tauri (tests): no progress events, results still arrive.
      listening = Promise.resolve(null);
    }
  }
  return listening;
}

/** Deliver a progress event as if it came from the backend (tests). */
export function dispatchJobProgress(p: JobProgress) {
  handlers.get(p.job)?.(p);
}

// Ids are made here so progress can be routed before the command returns.
let nextId = Math.floor(Math.random() * 1_000_000) * 1000;

// ---------------------------------------------------------------- running jobs store

let running: RunningJob[] = [];
const subscribers = new Set<() => void>();
const notify = () => subscribers.forEach((f) => f());

function setRunning(next: RunningJob[]) {
  running = next;
  notify();
}

function update(id: number, patch: Partial<RunningJob>) {
  if (!running.some((j) => j.id === id)) return;
  setRunning(running.map((j) => (j.id === id ? { ...j, ...patch } : j)));
}

/** The running jobs that asked to be shown (they have a label). */
export function useRunningJobs(): RunningJob[] {
  return useSyncExternalStore(
    (f) => {
      subscribers.add(f);
      return () => subscribers.delete(f);
    },
    () => running,
  );
}

export interface JobOptions {
  /** Shown in the status bar while running; leave out for jobs a dialog shows itself. */
  label?: string;
  /** Rows the work will read, for "n of total rows". */
  total?: number;
  onProgress?(p: JobProgress): void;
}

/**
 * Start a job: `run(id)` invokes the backend command with `job: id`.
 * The result rejects with kind "cancelled" when the job is cancelled.
 */
export function startJob<T>(run: (job: number) => Promise<T>, options: JobOptions = {}): Job<T> {
  const id = ++nextId;
  const cancel = () => void invoke("cancel_job", { job: id }).catch(() => undefined);
  handlers.set(id, (p) => {
    options.onProgress?.(p);
    if (options.label) {
      update(id, {
        ...(p.percent !== undefined ? { percent: p.percent } : {}),
        ...(p.rows !== undefined ? { rows: p.rows } : {}),
      });
    }
  });
  if (options.label) {
    setRunning([...running, { id, label: options.label, percent: null, rows: null, total: options.total ?? null, cancel }]);
  }
  const result = (async () => {
    // Listen before the work starts, so early progress is not lost.
    await listenOnce();
    return run(id);
  })().finally(() => {
    handlers.delete(id);
    if (options.label) setRunning(running.filter((j) => j.id !== id));
  });
  return { id, result, cancel };
}

/** "Stopped" answers are not errors: callers ignore them. */
export async function jobResult<T>(job: Job<T>): Promise<T | null> {
  try {
    return await job.result;
  } catch (e) {
    if (isCancelled(e)) return null;
    throw e;
  }
}

/** Fraction done (0–1) from rows read, or from DuckDB's percentage; null when unknown. */
export function jobFraction(j: { percent: number | null; rows: number | null; total: number | null }): number | null {
  if (j.rows !== null && j.total) return Math.min(1, j.rows / j.total);
  if (j.percent !== null && j.percent >= 0) return Math.min(1, j.percent / 100);
  return null;
}
