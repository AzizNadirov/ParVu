// Progress of a running job: a bar (moving when the amount is unknown), what
// is being done, rows read so far, and Stop. Inline in dialogs; compact in
// the status bar for jobs without a dialog.
import type { Translate } from "../lib/i18n";
import { jobFraction, useRunningJobs } from "../lib/jobs";

export interface JobProgressProps {
  t: Translate;
  label: string;
  percent?: number | null;
  rows?: number | null;
  total?: number | null;
  /** Extra line under the bar (e.g. "1,240 values so far"). */
  detail?: string;
  onCancel?(): void;
  compact?: boolean;
}

export function JobProgress({ t, label, percent = null, rows = null, total = null, detail, onCancel, compact }: JobProgressProps) {
  const fraction = jobFraction({ percent, rows, total });
  const counted =
    rows !== null && total ? t("job.rows", { rows, total }) : rows !== null ? t("job.rows_read", { rows }) : null;
  return (
    <div className={`pv-job ${compact ? "compact" : ""}`} role="status" aria-live="polite">
      <div className="pv-job-head">
        <span className="ellipsis grow">{label}</span>
        {fraction !== null && <span className="pv-job-pct">{Math.round(fraction * 100)}%</span>}
        {onCancel && (
          <button className="text small-btn" onClick={onCancel}>
            {t("job.stop")}
          </button>
        )}
      </div>
      <div
        className={`pv-progress ${fraction === null ? "indeterminate" : ""}`}
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={fraction !== null ? Math.round(fraction * 100) : undefined}
      >
        <div style={fraction !== null ? { width: `${Math.max(2, fraction * 100)}%` } : undefined} />
      </div>
      {!compact && (counted || detail) && (
        <div className="muted small">{[counted, detail].filter(Boolean).join(" · ")}</div>
      )}
    </div>
  );
}

/** The running jobs that have no dialog of their own, in the status bar. */
export function StatusJobs({ t }: { t: Translate }) {
  const jobs = useRunningJobs();
  if (!jobs.length) return null;
  const j = jobs[jobs.length - 1];
  const label = jobs.length > 1 ? `${j.label} (+${jobs.length - 1})` : j.label;
  return (
    <div className="pv-status-job">
      <JobProgress t={t} label={label} percent={j.percent} rows={j.rows} total={j.total} onCancel={j.cancel} compact />
    </div>
  );
}
