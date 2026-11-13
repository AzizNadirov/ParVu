# Crash Reporting and Logs

## Logs

ParVu writes a log to `~/.ParVu/logs/parvu.log.<date>`, one file per day. It records files opened, queries run, errors and warnings.

For more detail, start ParVu with a log level:

```bash
PARVU_LOG=debug parvu
```

## When ParVu crashes

If the app hits an unrecoverable error (a Rust panic), it writes a crash report before exiting:

- to the log, and
- to `~/.ParVu/logs/last_crash.txt`.

The report contains the ParVu version, the time, the operating system, the error message and a backtrace.

On the next start, ParVu shows the report in a dialog, then deletes `last_crash.txt`. The dialog can:

- **copy** the report to the clipboard,
- **open the logs folder**,
- **send by email**, which opens your mail app with the report addressed to `bug_report_email` from the settings.

Nothing is sent automatically. You decide what to share.

Errors that are not crashes, such as an invalid query or a file that cannot be read, are shown in a message and logged; they never close the app.

## Turning the dialog off

Set `"enable_crash_reporting": false` in `~/.ParVu/settings/settings.json`, or untick it in File → Settings. Crashes are still logged.

## Privacy

Logs and crash reports stay on your machine. They can contain file paths and query text, so read them before sharing.

## Reporting a bug

Please [open an issue](https://github.com/AzizNadirov/ParVu/issues) with:

- the crash report or the relevant part of the log,
- the steps that lead to the problem,
- the file type and roughly how large the file is.
