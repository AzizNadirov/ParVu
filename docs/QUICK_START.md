# ParVu Quick Start Guide

## Language Selection

### How to Change Language

The language selector is now integrated into ParVu! Here's how to use it:

1. **Open Settings**
   - Click `File` → `Settings...` in the menu bar
   - Or use the keyboard shortcut (if configured)

2. **Go to General Tab**
   - The Settings dialog has multiple tabs
   - Select the `General` tab (should be the first tab)

3. **Find Language Selector**
   - Scroll down in the General tab
   - Look for the **"Interface Language"** section
   - It's located below "File History"

4. **Select Your Language**
   - Click the dropdown menu
   - You'll see three options:
     - 🇬🇧 English (en)
     - 🇷🇺 Русский (ru)
     - 🇦🇿 Azərbaycan (az)

5. **Save Settings**
   - Click the `Save` button at the bottom of the dialog
   - Your language preference is saved

6. **Restart ParVu**
   - Close and reopen ParVu
   - The interface will now display in your selected language

## Settings Dialog Layout

```
┌─────────────────────────────────────────┐
│ Settings                           [X]   │
├─────────────────────────────────────────┤
│ [General] [Theme] [Advanced] [Warnings] │
├─────────────────────────────────────────┤
│                                          │
│ Data Settings                            │
│ ┌──────────────────────────────────────┐ │
│ │ Table Variable Name: [data         ]│ │
│ │ Rows Per Page:       [100          ]│ │
│ │ Max Rows (LIMIT):    [10000        ]│ │
│ └──────────────────────────────────────┘ │
│                                          │
│ File History                             │
│ ┌──────────────────────────────────────┐ │
│ │ ☑ Save File History                 │ │
│ └──────────────────────────────────────┘ │
│                                          │
│ Interface Language          ◄── HERE!   │
│ ┌──────────────────────────────────────┐ │
│ │ Language: [🇬🇧 English (en)    ▼]   │ │
│ │                                      │ │
│ │ Available languages:                 │ │
│ │  • 🇬🇧 English (en)                 │ │
│ │  • 🇷🇺 Русский (ru)                 │ │
│ │  • 🇦🇿 Azərbaycan (az)              │ │
│ └──────────────────────────────────────┘ │
│                                          │
├─────────────────────────────────────────┤
│              [Save] [Cancel]             │
└─────────────────────────────────────────┘
```

## What Gets Translated?

Currently, the i18n infrastructure is in place. When you select a language:

1. **Immediately**:
   - Language preference is saved
   - i18n system updates internally

2. **After Restart** (Full UI translation - pending integration):
   - Window titles
   - Menu items (File, Help, etc.)
   - Button labels (Save, Cancel, Browse, etc.)
   - Dialog messages
   - Error/warning messages
   - Status bar messages
   - Context menus
   - Tooltips

## Current Status

### ✅ Implemented
- Language selector in Settings → General
- 3 fully translated languages (180+ keys each)
- Language preference saving
- i18n system initialization

### ⏳ Pending (Future Updates)
- Full UI integration in all windows
- Real-time language switching (without restart)
- More languages (community contributions welcome!)

## Testing Your Language Selection

To verify your language is saved:

1. Open Settings
2. Change language to Russian (Русский)
3. Click Save
4. Check `~/.ParVu/settings/settings.json`
5. Look for: `"current_language": "ru"`

## Available Languages

| Flag | Language | Native Name | Code | Status |
|------|----------|-------------|------|--------|
| 🇬🇧 | English | English | en | ✅ Complete |
| 🇷🇺 | Russian | Русский | ru | ✅ Complete |
| 🇦🇿 | Azerbaijani | Azərbaycan | az | ✅ Complete |

## Language Files Location

Settings are stored in:
```
~/.ParVu/settings/settings.json
```

Key field:
```json
{
  "current_language": "en"
}
```

## Troubleshooting

### Language selector not visible
- Make sure you're in the **General** tab (first tab)
- Scroll down - it's below "File History"

### Language not changing
- Make sure you clicked **Save** (not Cancel)
- **Restart ParVu** - changes take effect on restart
- Check logs: `~/.ParVu/logs/parvu_*.log`

### Want to add a new language?
See [I18N.md](I18N.md#adding-a-new-language) for instructions.

## Screenshots

### English Interface
- Default language
- Clean, professional UI
- Comprehensive tooltips

### Russian Interface (Русский)
- Full Cyrillic support
- 180+ translated strings
- Professional terminology

### Azerbaijani Interface (Azərbaycan)
- Latin script with special characters
- Complete translation
- Native language support

## Using the Expression Language

ParVu supports a Python-like expression language for quick data transformations.

### Switch to Expression Mode

1. Click the **Expr / SQL** toggle in the query toolbar to switch to expression mode
2. The editor will now compile expressions to DuckDB SQL

### Add a Computed Column

Type in the expression editor:

```python
data[total] = price * quantity
```

Press **Execute** (or Ctrl+Enter). This adds a new column `total` computed from `price * quantity`.

### Remove Duplicates

Via the Operations menu:
1. Click **Operations → Drop Duplicates**
2. Select the columns to check for duplicates
3. Choose **Keep First** or **Keep Last**
4. Click **Apply**

Or via expression:

```python
DROP_DUPLICATES(data[id], data[name], 'first')
```

### Undo a Step

Every transform and cell edit is tracked in the **Applied Steps** panel:
1. Look at the panel below the data table
2. Click the **↩ Undo** button to revert the last step
3. Works for both SQL transforms and cell edits

## Need Help?

- **Documentation**: [docs/I18N.md](I18N.md)
- **Expression Language**: [docs/DSL.md](DSL.md)
- **GitHub Issues**: https://github.com/AzizNadirov/ParVu/issues
- **Telegram**: @aziz_nadirov

---

**Version**: 0.2.0+
**Last Updated**: 2026-05-26
