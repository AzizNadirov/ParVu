# ParVu Crash Reporting System

## Overview

ParVu includes a comprehensive crash reporting system that helps users report bugs and errors effectively. The system automatically logs all sessions and provides detailed error information when crashes occur.

## Features

### 1. Session-Based Logging

Each time ParVu starts, a new session log file is created with a unique identifier:

- **Log Location**: `~/.ParVu/logs/`
- **File Format**: `parvu_session_YYYYMMDD_HHMMSS_<uuid>.log`
- **Log Level**: DEBUG (captures detailed information)
- **Retention**: 30 days
- **Rotation**: 10 MB per file

Example log file:
```
parvu_session_20251230_201739_dc1a1a3f.log
```

### 2. Crash Report Dialog

When an unexpected error occurs, ParVu displays a user-friendly crash report dialog that includes:

- **Error Type**: The type of exception that occurred
- **Error Message**: Descriptive message about the error
- **System Information**: OS, platform, Python version
- **Full Traceback**: Complete stack trace for debugging
- **Log File Location**: Path to the session log file

### 3. User Actions

The crash report dialog provides the following options:

1. **Copy Error Details** - Copies all error information to clipboard
2. **Open Logs Folder** - Opens the logs directory in file explorer
3. **Close** - Dismisses the dialog

## Configuration

Crash reporting can be configured in settings:

### Settings File Location
`~/.ParVu/settings/settings.json`

### Configuration Options

```json
{
  "bug_report_email": "parvu.bugs@gmail.com",
  "enable_crash_reporting": true
}
```

- **bug_report_email**: Email address where bug reports should be sent
- **enable_crash_reporting**: Enable/disable crash report dialog (default: true)

## How to Report a Bug

When ParVu crashes:

1. The crash report dialog appears automatically
2. Click **"Copy Error Details"** to copy all error information
3. Click **"Open Logs Folder"** to access the session log file
4. Send an email to: **parvu.bugs@gmail.com** with:
   - The copied error details (paste from clipboard)
   - The session log file as an attachment
   - Description of what you were doing when the error occurred
   - Steps to reproduce the issue (if known)

## Log File Format

Session logs include detailed information with timestamps, log levels, and source locations:

```
2025-12-30 20:17:39.271 | INFO     | __main__:main:79 - Starting ParVu application - Session ID: 20251230_201739_dc1a1a3f
2025-12-30 20:17:39.272 | INFO     | __main__:main:80 - Session log file: /home/tengo/.ParVu/logs/parvu_session_20251230_201739_dc1a1a3f.log
```

Each log entry contains:
- Timestamp (millisecond precision)
- Log level (DEBUG, INFO, WARNING, ERROR, CRITICAL)
- Source module:function:line
- Log message

## Implementation Details

### Exception Handling

ParVu implements multiple layers of exception handling:

1. **Global Exception Hook** (`sys.excepthook`)
   - Catches unhandled exceptions at the system level
   - Logs the exception with full traceback
   - Displays crash report dialog

2. **Main Application Try-Except**
   - Wraps the main application event loop
   - Catches exceptions during runtime
   - Ensures graceful error handling

3. **Detailed Logging**
   - `backtrace=True`: Includes variable values in tracebacks
   - `diagnose=True`: Adds diagnostic information
   - Rotating log files to prevent disk space issues

### File Structure

```
~/.ParVu/
├── logs/
│   ├── parvu_session_20251230_201739_dc1a1a3f.log
│   ├── parvu_session_20251230_183045_a7b2c1d4.log
│   └── ...
└── settings/
    └── settings.json
```

## Testing Crash Reporting

A test script is provided to verify crash reporting functionality:

```bash
python test_crash.py
```

This script:
- Creates a test window with a button
- Clicking the button triggers a simulated crash
- Displays the crash report dialog
- Allows testing of all dialog features

## Privacy Considerations

- Logs are stored **locally only** on the user's machine
- No automatic transmission of crash data
- Users manually choose what to report and when
- Log files can contain file paths and query data
- Users should review logs before sharing

## Disabling Crash Reporting

To disable crash reporting:

1. Edit `~/.ParVu/settings/settings.json`
2. Set `"enable_crash_reporting": false`
3. Restart ParVu

When disabled:
- Errors are still logged to session files
- Crash report dialog is not shown
- Application exits silently on fatal errors

## Best Practices

For users reporting bugs:
- Include the complete session log file
- Describe steps to reproduce the issue
- Mention the file type and size you were working with
- Note any recent changes to settings or themes

For developers:
- Check session logs for detailed error context
- Look for patterns in multiple crash reports
- Verify system information matches testing environment
- Use DEBUG level logs for troubleshooting
