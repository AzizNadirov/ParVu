"""
Internationalization (i18n) System for ParVu
Supports multiple languages with easy extensibility
"""
from typing import Dict, Optional
from pathlib import Path
from loguru import logger


class Locale:
    """Represents a language locale with translations"""

    def __init__(self, code: str, name: str, native_name: str, flag: str):
        self.code = code  # ISO 639-1 code (e.g., 'en', 'ru', 'az')
        self.name = name  # English name
        self.native_name = native_name  # Native name
        self.flag = flag  # Flag emoji
        self.translations: Dict[str, str] = {}

    def translate(self, key: str, **kwargs) -> str:
        """Get translation for key with optional formatting"""
        text = self.translations.get(key, key)
        if kwargs:
            try:
                return text.format(**kwargs)
            except (KeyError, ValueError) as e:
                logger.warning(f"Translation formatting error for key '{key}': {e}")
                return text
        return text

    def __repr__(self):
        return f"Locale({self.code}, {self.native_name})"


class I18n:
    """Internationalization manager"""

    def __init__(self):
        self.locales: Dict[str, Locale] = {}
        self.current_locale: Optional[Locale] = None
        self._initialize_locales()

    def _initialize_locales(self):
        """Initialize all supported locales"""
        # English
        en = Locale("en", "English", "English", "🇬🇧")
        en.translations = TRANSLATIONS_EN
        self.locales["en"] = en

        # Russian
        ru = Locale("ru", "Russian", "Русский", "🇷🇺")
        ru.translations = TRANSLATIONS_RU
        self.locales["ru"] = ru

        # Azerbaijani
        az = Locale("az", "Azerbaijani", "Azərbaycan", "🇦🇿")
        az.translations = TRANSLATIONS_AZ
        self.locales["az"] = az

        # Set default locale
        self.current_locale = en
        logger.info(f"Initialized {len(self.locales)} locales")

    def set_locale(self, code: str) -> bool:
        """Set current locale by code"""
        if code in self.locales:
            self.current_locale = self.locales[code]
            logger.info(f"Locale set to: {self.current_locale.native_name} ({code})")
            return True
        logger.warning(f"Locale not found: {code}")
        return False

    def get_available_locales(self) -> list[Locale]:
        """Get list of available locales"""
        return list(self.locales.values())

    def t(self, key: str, **kwargs) -> str:
        """Translate key (shorthand)"""
        if self.current_locale:
            return self.current_locale.translate(key, **kwargs)
        return key


# Translation keys for English
TRANSLATIONS_EN = {
    # Window & Application
    "app.name": "ParVu",
    "app.title": "ParVu - Parquet/CSV Viewer",
    "app.ready": "Ready",
    "app.loading": "Loading...",

    # Menu Bar
    "menu.file": "File",
    "menu.file.new_window": "New Window",
    "menu.file.open": "Open File...",
    "menu.file.export": "Export Results...",
    "menu.file.settings": "Settings...",
    "menu.file.change_theme": "Change Theme...",
    "menu.file.recent_files": "Recent Files",
    "menu.file.clear_recents": "Clear List",
    "menu.file.exit": "Exit",
    "menu.help": "Help",
    "menu.help.about": "About ParVu",

    # Buttons
    "btn.browse": "Browse & Load...",
    "btn.reload": "Reload from Path",
    "btn.execute": "Execute Query",
    "btn.reset": "Reset Query",
    "btn.table_info": "Table Info",
    "btn.previous": "◀ Previous",
    "btn.next": "Next ▶",
    "btn.close": "Close",
    "btn.save": "Save",
    "btn.cancel": "Cancel",
    "btn.apply": "Apply",
    "btn.import": "Import...",
    "btn.export": "Export...",
    "btn.delete": "Delete",
    "btn.select_all": "Select All",
    "btn.deselect_all": "Deselect All",
    "btn.apply_filter": "Apply Filter",

    # Labels & Placeholders
    "label.file_placeholder": "Select a Parquet, CSV, or JSON file...",
    "label.sql_query": "SQL Query (Table: {table_name}):",
    "label.results": "Results:",
    "label.page": "Page: -",
    "label.page_info": "Page: {page}",
    "label.search_placeholder": "Search values...",

    # Status Messages
    "status.ready": "Ready",
    "status.loading": "Loading...",
    "status.page_info": "Showing page {page} of {total_pages}",
    "status.query_failed": "Query failed",
    "status.sorted": "Sorted by {column} ({direction})",
    "status.loaded": "Loaded: {filename} ({rows:,} rows)",
    "status.calculating": "Calculating unique values...",

    # Dialog Titles
    "dialog.open_file": "Open File",
    "dialog.table_info": "Table Information",
    "dialog.about": "About ParVu",
    "dialog.export_results": "Export Results",
    "dialog.settings": "Settings",
    "dialog.theme_selector": "Theme Selector",
    "dialog.unique_values": "Unique Values - {column_name}",
    "dialog.import_theme": "Import Theme",
    "dialog.export_theme": "Export Theme",

    # Error Messages
    "error.no_file": "No File",
    "error.no_file_msg": "Please enter or browse for a file.",
    "error.file_not_found": "File Not Found",
    "error.file_not_found_msg": "File does not exist: {path}",
    "error.load_error": "Load Error",
    "error.load_error_msg": "Failed to load file: {error}",
    "error.query_error": "Query Error",
    "error.query_error_msg": "Query Error:\n\n{error_msg}",
    "error.empty_query": "Empty Query",
    "error.empty_query_msg": "Please enter a SQL query.",
    "error.sort_failed": "Sort Failed",
    "error.sort_failed_msg": "Failed to sort by {column}:\n{error_msg}",
    "error.unique_values_error": "Error",
    "error.unique_values_error_msg": "Failed to get unique values: {error}",
    "error.no_values": "No Values",
    "error.no_values_msg": "No unique values found for {column}",
    "error.no_selection": "No Selection",
    "error.no_selection_msg": "Please select at least one value to filter.",
    "error.export_error": "Export Error",
    "error.export_error_msg": "Error: {error}",

    # Warning Messages
    "warning.no_data": "No Data",
    "warning.no_data_msg": "Please load a file first.",
    "warning.large_dataset": "Large Dataset Warning",
    "warning.large_dataset_rows": "This dataset has {rows:,} rows (threshold: {threshold:,}).\nCalculating unique values may take some time.\n\nContinue?",
    "warning.large_dataset_cells": "This dataset has {cells:,} cells ({rows:,} rows × {columns} columns, threshold: {threshold:,}).\nCalculating unique values may take some time.\n\nContinue?",
    "warning.large_dataset_size": "This file is {size:.1f} MB (threshold: {threshold} MB).\nCalculating unique values may take some time.\n\nContinue?",
    "warning.file_not_found_recent": "File not found:\n{path}\n\nRemove from recents?",

    # Success Messages
    "success.export_complete": "Export Complete",
    "success.export_complete_msg": "Results exported to:\n{path}",
    "success.export_failed": "Export Failed",
    "success.export_failed_msg": "Failed to export results.",
    "success.import_theme": "Import Successful",
    "success.import_theme_msg": "Theme '{name}' imported successfully!",
    "success.import_failed": "Import Failed",
    "success.import_failed_msg": "Failed to import theme. Check the file format.",
    "success.export_theme": "Export Successful",
    "success.export_theme_msg": "Theme exported to:\n{path}",
    "success.theme_deleted": "Theme Deleted",
    "success.theme_deleted_msg": "Theme '{name}' deleted successfully.",

    # Settings Dialog
    "settings.tab.general": "General",
    "settings.tab.theme": "Theme",
    "settings.tab.advanced": "Advanced",
    "settings.tab.warnings": "Warnings",

    "settings.group.data": "Data Settings",
    "settings.group.file_history": "File History",
    "settings.group.theme_selection": "Theme Selection",
    "settings.group.theme_preview": "Theme Preview:",
    "settings.group.fonts": "Default Fonts",
    "settings.group.sql": "SQL Settings",
    "settings.group.large_dataset": "Large Dataset Warning",
    "settings.group.warning_criteria": "Warning Criteria",

    "settings.label.table_var": "Table Variable Name:",
    "settings.label.rows_per_page": "Rows Per Page:",
    "settings.label.max_rows": "Max Rows (LIMIT):",
    "settings.label.save_history": "Save File History:",
    "settings.label.active_theme": "Active Theme:",
    "settings.label.sql_font": "SQL Editor Font:",
    "settings.label.sql_font_size": "SQL Font Size:",
    "settings.label.table_font_size": "Table Font Size:",
    "settings.label.default_query": "Default Query:",
    "settings.label.default_limit": "Default LIMIT:",

    "settings.tooltip.table_var": "Name used for table in SQL queries",
    "settings.tooltip.rows_per_page": "Number of rows to display per page",
    "settings.tooltip.max_rows": "Maximum rows allowed in LIMIT clause",
    "settings.tooltip.save_history": "Save recently opened files",
    "settings.tooltip.sql_font": "Font family for SQL editor",
    "settings.tooltip.sql_font_size": "Font size for SQL editor",
    "settings.tooltip.table_font_size": "Font size for table data",
    "settings.tooltip.default_query": "Default SQL query template",
    "settings.tooltip.default_limit": "Default LIMIT value",
    "settings.tooltip.enable_warning": "Show warning dialog before calculating unique values on large datasets",
    "settings.tooltip.row_threshold": "Warn when row count exceeds this value",
    "settings.tooltip.cell_threshold": "Warn when total cell count (rows × columns) exceeds this value",
    "settings.tooltip.size_threshold": "Warn when file size exceeds this value",

    "settings.warning.enable": "Enable warning when loading unique values on large datasets",
    "settings.warning.trigger": "Trigger warning based on:",
    "settings.warning.row_count": "Row count",
    "settings.warning.cell_count": "Cell count (rows × columns)",
    "settings.warning.file_size": "File size",
    "settings.warning.threshold": "Threshold:",

    "settings.suffix.rows": " rows",
    "settings.suffix.cells": " cells",
    "settings.suffix.mb": " MB",

    "settings.note.fonts": "Note: Theme fonts and colors will override these defaults.\nChange theme in the Theme tab for comprehensive styling.",
    "settings.note.warnings": "Note: Large dataset warnings help prevent long calculation times when requesting unique values for columns in very large files. The warning allows you to cancel before potentially slow operations begin.",

    "settings.error.invalid": "Invalid Setting",
    "settings.error.invalid_msg": "Table variable name cannot be empty or a SQL keyword.",
    "settings.error.save_failed": "Save Failed",
    "settings.error.save_failed_msg": "Failed to save settings:\n{error}",

    # Theme Selector
    "theme.selector.title": "Select a theme for ParVu",
    "theme.selector.available": "Available Themes:",
    "theme.selector.preview": "Preview",
    "theme.selector.delete_confirm": "Delete Theme",
    "theme.selector.delete_confirm_msg": "Are you sure you want to delete the theme '{name}'?\n\nThis action cannot be undone.",
    "theme.selector.delete_failed": "Delete Failed",
    "theme.selector.delete_failed_msg": "Cannot delete built-in themes.",
    "theme.selector.no_selection": "No Theme Selected",
    "theme.selector.no_selection_msg": "Please select a theme to export.",

    # Context Menu
    "context.copy_column": "Copy Column Name",
    "context.sort_asc": "Sort Ascending",
    "context.sort_desc": "Sort Descending",
    "context.copy_values": "Copy Values as Tuple",
    "context.unique_values": "Show Unique Values...",

    # Unique Values Dialog
    "unique.found": "Found {count} unique values (select multiple)",

    # Language Settings
    "language.name": "Language:",
    "language.label": "Interface Language:",
}


# Translation keys for Russian
TRANSLATIONS_RU = {
    # Window & Application
    "app.name": "ParVu",
    "app.title": "ParVu - Просмотр Parquet/CSV",
    "app.ready": "Готово",
    "app.loading": "Загрузка...",

    # Menu Bar
    "menu.file": "Файл",
    "menu.file.new_window": "Новое окно",
    "menu.file.open": "Открыть файл...",
    "menu.file.export": "Экспортировать результаты...",
    "menu.file.settings": "Настройки...",
    "menu.file.change_theme": "Изменить тему...",
    "menu.file.recent_files": "Последние файлы",
    "menu.file.clear_recents": "Очистить список",
    "menu.file.exit": "Выход",
    "menu.help": "Справка",
    "menu.help.about": "О программе ParVu",

    # Buttons
    "btn.browse": "Обзор и загрузка...",
    "btn.reload": "Перезагрузить",
    "btn.execute": "Выполнить запрос",
    "btn.reset": "Сбросить запрос",
    "btn.table_info": "Инфо о таблице",
    "btn.previous": "◀ Пред",
    "btn.next": "След ▶",
    "btn.close": "Закрыть",
    "btn.save": "Сохранить",
    "btn.cancel": "Отмена",
    "btn.apply": "Применить",
    "btn.import": "Импорт...",
    "btn.export": "Экспорт...",
    "btn.delete": "Удалить",
    "btn.select_all": "Выбрать все",
    "btn.deselect_all": "Снять выбор",
    "btn.apply_filter": "Применить фильтр",

    # Labels & Placeholders
    "label.file_placeholder": "Выберите файл Parquet, CSV или JSON...",
    "label.sql_query": "SQL запрос (Таблица: {table_name}):",
    "label.results": "Результаты:",
    "label.page": "Страница: -",
    "label.page_info": "Страница: {page}",
    "label.search_placeholder": "Поиск значений...",

    # Status Messages
    "status.ready": "Готово",
    "status.loading": "Загрузка...",
    "status.page_info": "Страница {page} из {total_pages}",
    "status.query_failed": "Ошибка запроса",
    "status.sorted": "Отсортировано по {column} ({direction})",
    "status.loaded": "Загружено: {filename} ({rows:,} строк)",
    "status.calculating": "Расчёт уникальных значений...",

    # Dialog Titles
    "dialog.open_file": "Открыть файл",
    "dialog.table_info": "Информация о таблице",
    "dialog.about": "О программе ParVu",
    "dialog.export_results": "Экспорт результатов",
    "dialog.settings": "Настройки",
    "dialog.theme_selector": "Выбор темы",
    "dialog.unique_values": "Уникальные значения - {column_name}",
    "dialog.import_theme": "Импорт темы",
    "dialog.export_theme": "Экспорт темы",

    # Error Messages
    "error.no_file": "Нет файла",
    "error.no_file_msg": "Пожалуйста, выберите файл.",
    "error.file_not_found": "Файл не найден",
    "error.file_not_found_msg": "Файл не существует: {path}",
    "error.load_error": "Ошибка загрузки",
    "error.load_error_msg": "Не удалось загрузить файл: {error}",
    "error.query_error": "Ошибка запроса",
    "error.query_error_msg": "Ошибка запроса:\n\n{error_msg}",
    "error.empty_query": "Пустой запрос",
    "error.empty_query_msg": "Пожалуйста, введите SQL запрос.",
    "error.sort_failed": "Ошибка сортировки",
    "error.sort_failed_msg": "Не удалось отсортировать по {column}:\n{error_msg}",
    "error.unique_values_error": "Ошибка",
    "error.unique_values_error_msg": "Не удалось получить уникальные значения: {error}",
    "error.no_values": "Нет значений",
    "error.no_values_msg": "Уникальные значения не найдены для {column}",
    "error.no_selection": "Нет выбора",
    "error.no_selection_msg": "Пожалуйста, выберите хотя бы одно значение для фильтрации.",
    "error.export_error": "Ошибка экспорта",
    "error.export_error_msg": "Ошибка: {error}",

    # Warning Messages
    "warning.no_data": "Нет данных",
    "warning.no_data_msg": "Пожалуйста, сначала загрузите файл.",
    "warning.large_dataset": "Предупреждение о большом наборе данных",
    "warning.large_dataset_rows": "Этот набор данных содержит {rows:,} строк (порог: {threshold:,}).\nРасчёт уникальных значений может занять время.\n\nПродолжить?",
    "warning.large_dataset_cells": "Этот набор данных содержит {cells:,} ячеек ({rows:,} строк × {columns} столбцов, порог: {threshold:,}).\nРасчёт уникальных значений может занять время.\n\nПродолжить?",
    "warning.large_dataset_size": "Размер этого файла {size:.1f} МБ (порог: {threshold} МБ).\nРасчёт уникальных значений может занять время.\n\nПродолжить?",
    "warning.file_not_found_recent": "Файл не найден:\n{path}\n\nУдалить из последних?",

    # Success Messages
    "success.export_complete": "Экспорт завершён",
    "success.export_complete_msg": "Результаты экспортированы в:\n{path}",
    "success.export_failed": "Ошибка экспорта",
    "success.export_failed_msg": "Не удалось экспортировать результаты.",
    "success.import_theme": "Импорт успешен",
    "success.import_theme_msg": "Тема '{name}' успешно импортирована!",
    "success.import_failed": "Ошибка импорта",
    "success.import_failed_msg": "Не удалось импортировать тему. Проверьте формат файла.",
    "success.export_theme": "Экспорт успешен",
    "success.export_theme_msg": "Тема экспортирована в:\n{path}",
    "success.theme_deleted": "Тема удалена",
    "success.theme_deleted_msg": "Тема '{name}' успешно удалена.",

    # Settings Dialog
    "settings.tab.general": "Общие",
    "settings.tab.theme": "Тема",
    "settings.tab.advanced": "Расширенные",
    "settings.tab.warnings": "Предупреждения",

    "settings.group.data": "Настройки данных",
    "settings.group.file_history": "История файлов",
    "settings.group.theme_selection": "Выбор темы",
    "settings.group.theme_preview": "Предпросмотр темы:",
    "settings.group.fonts": "Шрифты по умолчанию",
    "settings.group.sql": "Настройки SQL",
    "settings.group.large_dataset": "Предупреждение о больших данных",
    "settings.group.warning_criteria": "Критерии предупреждения",

    "settings.label.table_var": "Имя переменной таблицы:",
    "settings.label.rows_per_page": "Строк на странице:",
    "settings.label.max_rows": "Макс. строк (LIMIT):",
    "settings.label.save_history": "Сохранять историю:",
    "settings.label.active_theme": "Активная тема:",
    "settings.label.sql_font": "Шрифт SQL редактора:",
    "settings.label.sql_font_size": "Размер шрифта SQL:",
    "settings.label.table_font_size": "Размер шрифта таблицы:",
    "settings.label.default_query": "Запрос по умолчанию:",
    "settings.label.default_limit": "LIMIT по умолчанию:",

    "settings.tooltip.table_var": "Имя таблицы в SQL запросах",
    "settings.tooltip.rows_per_page": "Количество строк на странице",
    "settings.tooltip.max_rows": "Максимальное количество строк в LIMIT",
    "settings.tooltip.save_history": "Сохранять недавно открытые файлы",
    "settings.tooltip.sql_font": "Семейство шрифтов для SQL редактора",
    "settings.tooltip.sql_font_size": "Размер шрифта для SQL редактора",
    "settings.tooltip.table_font_size": "Размер шрифта для данных таблицы",
    "settings.tooltip.default_query": "Шаблон SQL запроса по умолчанию",
    "settings.tooltip.default_limit": "Значение LIMIT по умолчанию",
    "settings.tooltip.enable_warning": "Показывать предупреждение перед расчётом уникальных значений в больших данных",
    "settings.tooltip.row_threshold": "Предупреждать при превышении количества строк",
    "settings.tooltip.cell_threshold": "Предупреждать при превышении количества ячеек (строки × столбцы)",
    "settings.tooltip.size_threshold": "Предупреждать при превышении размера файла",

    "settings.warning.enable": "Включить предупреждение при загрузке уникальных значений больших данных",
    "settings.warning.trigger": "Включать предупреждение при:",
    "settings.warning.row_count": "Количество строк",
    "settings.warning.cell_count": "Количество ячеек (строки × столбцы)",
    "settings.warning.file_size": "Размер файла",
    "settings.warning.threshold": "Порог:",

    "settings.suffix.rows": " строк",
    "settings.suffix.cells": " ячеек",
    "settings.suffix.mb": " МБ",

    "settings.note.fonts": "Примечание: Шрифты и цвета темы переопределят эти настройки по умолчанию.\nИзмените тему во вкладке Тема для комплексной настройки стиля.",
    "settings.note.warnings": "Примечание: Предупреждения о больших данных помогают предотвратить длительные расчёты при запросе уникальных значений столбцов в очень больших файлах. Предупреждение позволяет отменить операцию до начала потенциально медленных вычислений.",

    "settings.error.invalid": "Неверная настройка",
    "settings.error.invalid_msg": "Имя переменной таблицы не может быть пустым или ключевым словом SQL.",
    "settings.error.save_failed": "Ошибка сохранения",
    "settings.error.save_failed_msg": "Не удалось сохранить настройки:\n{error}",

    # Theme Selector
    "theme.selector.title": "Выберите тему для ParVu",
    "theme.selector.available": "Доступные темы:",
    "theme.selector.preview": "Предпросмотр",
    "theme.selector.delete_confirm": "Удалить тему",
    "theme.selector.delete_confirm_msg": "Вы уверены, что хотите удалить тему '{name}'?\n\nЭто действие нельзя отменить.",
    "theme.selector.delete_failed": "Ошибка удаления",
    "theme.selector.delete_failed_msg": "Нельзя удалить встроенные темы.",
    "theme.selector.no_selection": "Тема не выбрана",
    "theme.selector.no_selection_msg": "Пожалуйста, выберите тему для экспорта.",

    # Context Menu
    "context.copy_column": "Копировать имя столбца",
    "context.sort_asc": "Сортировать по возрастанию",
    "context.sort_desc": "Сортировать по убыванию",
    "context.copy_values": "Копировать значения как кортеж",
    "context.unique_values": "Показать уникальные значения...",

    # Unique Values Dialog
    "unique.found": "Найдено {count} уникальных значений (множественный выбор)",

    # Language Settings
    "language.name": "Язык:",
    "language.label": "Язык интерфейса:",
}


# Translation keys for Azerbaijani
TRANSLATIONS_AZ = {
    # Window & Application
    "app.name": "ParVu",
    "app.title": "ParVu - Parquet/CSV Baxışçısı",
    "app.ready": "Hazır",
    "app.loading": "Yüklənir...",

    # Menu Bar
    "menu.file": "Fayl",
    "menu.file.new_window": "Yeni pəncərə",
    "menu.file.open": "Fayl aç...",
    "menu.file.export": "Nəticələri ixrac et...",
    "menu.file.settings": "Parametrlər...",
    "menu.file.change_theme": "Mövzunu dəyişdir...",
    "menu.file.recent_files": "Son fayllar",
    "menu.file.clear_recents": "Siyahını təmizlə",
    "menu.file.exit": "Çıxış",
    "menu.help": "Kömək",
    "menu.help.about": "ParVu haqqında",

    # Buttons
    "btn.browse": "Bax və yüklə...",
    "btn.reload": "Yenidən yüklə",
    "btn.execute": "Sorğunu icra et",
    "btn.reset": "Sorğunu sıfırla",
    "btn.table_info": "Cədvəl məlumatı",
    "btn.previous": "◀ Əvvəlki",
    "btn.next": "Növbəti ▶",
    "btn.close": "Bağla",
    "btn.save": "Saxla",
    "btn.cancel": "Ləğv et",
    "btn.apply": "Tətbiq et",
    "btn.import": "İdxal...",
    "btn.export": "İxrac...",
    "btn.delete": "Sil",
    "btn.select_all": "Hamısını seç",
    "btn.deselect_all": "Seçimi ləğv et",
    "btn.apply_filter": "Filtri tətbiq et",

    # Labels & Placeholders
    "label.file_placeholder": "Parquet, CSV və ya JSON faylı seçin...",
    "label.sql_query": "SQL sorğusu (Cədvəl: {table_name}):",
    "label.results": "Nəticələr:",
    "label.page": "Səhifə: -",
    "label.page_info": "Səhifə: {page}",
    "label.search_placeholder": "Dəyərləri axtar...",

    # Status Messages
    "status.ready": "Hazır",
    "status.loading": "Yüklənir...",
    "status.page_info": "{total_pages} səhifədən {page} göstərilir",
    "status.query_failed": "Sorğu uğursuz oldu",
    "status.sorted": "{column} üzrə çeşidləndi ({direction})",
    "status.loaded": "Yükləndi: {filename} ({rows:,} sətir)",
    "status.calculating": "Unikal dəyərlər hesablanır...",

    # Dialog Titles
    "dialog.open_file": "Fayl aç",
    "dialog.table_info": "Cədvəl məlumatı",
    "dialog.about": "ParVu haqqında",
    "dialog.export_results": "Nəticələri ixrac et",
    "dialog.settings": "Parametrlər",
    "dialog.theme_selector": "Mövzu seçici",
    "dialog.unique_values": "Unikal dəyərlər - {column_name}",
    "dialog.import_theme": "Mövzunu idxal et",
    "dialog.export_theme": "Mövzunu ixrac et",

    # Error Messages
    "error.no_file": "Fayl yoxdur",
    "error.no_file_msg": "Zəhmət olmasa fayl seçin.",
    "error.file_not_found": "Fayl tapılmadı",
    "error.file_not_found_msg": "Fayl mövcud deyil: {path}",
    "error.load_error": "Yükləmə xətası",
    "error.load_error_msg": "Faylı yükləmək mümkün olmadı: {error}",
    "error.query_error": "Sorğu xətası",
    "error.query_error_msg": "Sorğu xətası:\n\n{error_msg}",
    "error.empty_query": "Boş sorğu",
    "error.empty_query_msg": "Zəhmət olmasa SQL sorğusu daxil edin.",
    "error.sort_failed": "Çeşidləmə uğursuz oldu",
    "error.sort_failed_msg": "{column} üzrə çeşidləmə uğursuz oldu:\n{error_msg}",
    "error.unique_values_error": "Xəta",
    "error.unique_values_error_msg": "Unikal dəyərləri əldə etmək mümkün olmadı: {error}",
    "error.no_values": "Dəyər yoxdur",
    "error.no_values_msg": "{column} üçün unikal dəyər tapılmadı",
    "error.no_selection": "Seçim yoxdur",
    "error.no_selection_msg": "Zəhmət olmasa filtr üçün ən azı bir dəyər seçin.",
    "error.export_error": "İxrac xətası",
    "error.export_error_msg": "Xəta: {error}",

    # Warning Messages
    "warning.no_data": "Məlumat yoxdur",
    "warning.no_data_msg": "Zəhmət olmasa əvvəlcə fayl yükləyin.",
    "warning.large_dataset": "Böyük məlumat dəsti xəbərdarlığı",
    "warning.large_dataset_rows": "Bu məlumat dəstində {rows:,} sətir var (hədd: {threshold:,}).\nUnikal dəyərlərin hesablanması vaxt ala bilər.\n\nDavam edək?",
    "warning.large_dataset_cells": "Bu məlumat dəstində {cells:,} xana var ({rows:,} sətir × {columns} sütun, hədd: {threshold:,}).\nUnikal dəyərlərin hesablanması vaxt ala bilər.\n\nDavam edək?",
    "warning.large_dataset_size": "Bu faylın ölçüsü {size:.1f} MB-dır (hədd: {threshold} MB).\nUnikal dəyərlərin hesablanması vaxt ala bilər.\n\nDavam edək?",
    "warning.file_not_found_recent": "Fayl tapılmadı:\n{path}\n\nSon fayllardan silinsin?",

    # Success Messages
    "success.export_complete": "İxrac tamamlandı",
    "success.export_complete_msg": "Nəticələr ixrac edildi:\n{path}",
    "success.export_failed": "İxrac uğursuz oldu",
    "success.export_failed_msg": "Nəticələri ixrac etmək mümkün olmadı.",
    "success.import_theme": "İdxal uğurlu oldu",
    "success.import_theme_msg": "'{name}' mövzusu uğurla idxal edildi!",
    "success.import_failed": "İdxal uğursuz oldu",
    "success.import_failed_msg": "Mövzunu idxal etmək mümkün olmadı. Fayl formatını yoxlayın.",
    "success.export_theme": "İxrac uğurlu oldu",
    "success.export_theme_msg": "Mövzu ixrac edildi:\n{path}",
    "success.theme_deleted": "Mövzu silindi",
    "success.theme_deleted_msg": "'{name}' mövzusu uğurla silindi.",

    # Settings Dialog
    "settings.tab.general": "Ümumi",
    "settings.tab.theme": "Mövzu",
    "settings.tab.advanced": "Əlavə",
    "settings.tab.warnings": "Xəbərdarlıqlar",

    "settings.group.data": "Məlumat parametrləri",
    "settings.group.file_history": "Fayl tarixçəsi",
    "settings.group.theme_selection": "Mövzu seçimi",
    "settings.group.theme_preview": "Mövzu önbaxışı:",
    "settings.group.fonts": "Standart şriftlər",
    "settings.group.sql": "SQL parametrləri",
    "settings.group.large_dataset": "Böyük məlumat dəsti xəbərdarlığı",
    "settings.group.warning_criteria": "Xəbərdarlıq meyarları",

    "settings.label.table_var": "Cədvəl dəyişəninin adı:",
    "settings.label.rows_per_page": "Səhifə başına sətir:",
    "settings.label.max_rows": "Maks. sətir (LIMIT):",
    "settings.label.save_history": "Tarixçəni saxla:",
    "settings.label.active_theme": "Aktiv mövzu:",
    "settings.label.sql_font": "SQL redaktor şrifti:",
    "settings.label.sql_font_size": "SQL şrift ölçüsü:",
    "settings.label.table_font_size": "Cədvəl şrift ölçüsü:",
    "settings.label.default_query": "Standart sorğu:",
    "settings.label.default_limit": "Standart LIMIT:",

    "settings.tooltip.table_var": "SQL sorğularında cədvəl üçün istifadə olunan ad",
    "settings.tooltip.rows_per_page": "Səhifədə göstəriləcək sətir sayı",
    "settings.tooltip.max_rows": "LIMIT bəndində icazə verilən maksimum sətir sayı",
    "settings.tooltip.save_history": "Son açılmış faylları saxla",
    "settings.tooltip.sql_font": "SQL redaktor üçün şrift ailəsi",
    "settings.tooltip.sql_font_size": "SQL redaktor üçün şrift ölçüsü",
    "settings.tooltip.table_font_size": "Cədvəl məlumatı üçün şrift ölçüsü",
    "settings.tooltip.default_query": "Standart SQL sorğu şablonu",
    "settings.tooltip.default_limit": "Standart LIMIT dəyəri",
    "settings.tooltip.enable_warning": "Böyük məlumat dəstlərində unikal dəyərləri hesablamadan əvvəl xəbərdarlıq dialoqu göstər",
    "settings.tooltip.row_threshold": "Sətir sayı bu dəyəri aşdıqda xəbərdarlıq et",
    "settings.tooltip.cell_threshold": "Ümumi xana sayı (sətir × sütun) bu dəyəri aşdıqda xəbərdarlıq et",
    "settings.tooltip.size_threshold": "Fayl ölçüsü bu dəyəri aşdıqda xəbərdarlıq et",

    "settings.warning.enable": "Böyük məlumat dəstlərində unikal dəyərləri yükləyərkən xəbərdarlığı aktivləşdir",
    "settings.warning.trigger": "Xəbərdarlığı aktivləşdir:",
    "settings.warning.row_count": "Sətir sayı",
    "settings.warning.cell_count": "Xana sayı (sətir × sütun)",
    "settings.warning.file_size": "Fayl ölçüsü",
    "settings.warning.threshold": "Hədd:",

    "settings.suffix.rows": " sətir",
    "settings.suffix.cells": " xana",
    "settings.suffix.mb": " MB",

    "settings.note.fonts": "Qeyd: Mövzu şriftləri və rəngləri bu standart parametrləri ləğv edəcək.\nKompleks üslublaşdırma üçün Mövzu sekmesində mövzunu dəyişdirin.",
    "settings.note.warnings": "Qeyd: Böyük məlumat dəsti xəbərdarlıqları çox böyük fayllarda sütunlar üçün unikal dəyərlər tələb edərkən uzun hesablama vaxtlarının qarşısını almağa kömək edir. Xəbərdarlıq potensial yavaş əməliyyatlar başlamazdan əvvəl ləğv etməyə imkan verir.",

    "settings.error.invalid": "Yanlış parametr",
    "settings.error.invalid_msg": "Cədvəl dəyişəninin adı boş və ya SQL açar sözü ola bilməz.",
    "settings.error.save_failed": "Saxlama uğursuz oldu",
    "settings.error.save_failed_msg": "Parametrləri saxlamaq mümkün olmadı:\n{error}",

    # Theme Selector
    "theme.selector.title": "ParVu üçün mövzu seçin",
    "theme.selector.available": "Mövcud mövzular:",
    "theme.selector.preview": "Önbaxış",
    "theme.selector.delete_confirm": "Mövzunu sil",
    "theme.selector.delete_confirm_msg": "'{name}' mövzusunu silmək istədiyinizdən əminsiniz?\n\nBu əməliyyat geri alına bilməz.",
    "theme.selector.delete_failed": "Silinmə uğursuz oldu",
    "theme.selector.delete_failed_msg": "Daxili mövzuları silmək mümkün deyil.",
    "theme.selector.no_selection": "Mövzu seçilməyib",
    "theme.selector.no_selection_msg": "Zəhmət olmasa ixrac üçün mövzu seçin.",

    # Context Menu
    "context.copy_column": "Sütun adını kopyala",
    "context.sort_asc": "Artan sıra ilə çeşidlə",
    "context.sort_desc": "Azalan sıra ilə çeşidlə",
    "context.copy_values": "Dəyərləri tuple kimi kopyala",
    "context.unique_values": "Unikal dəyərləri göstər...",

    # Unique Values Dialog
    "unique.found": "{count} unikal dəyər tapıldı (çoxlu seçim)",

    # Language Settings
    "language.name": "Dil:",
    "language.label": "İnterfeys dili:",
}


# Global i18n instance
_i18n = I18n()


def get_i18n() -> I18n:
    """Get the global i18n instance"""
    return _i18n


def t(key: str, **kwargs) -> str:
    """Shorthand for translate"""
    return _i18n.t(key, **kwargs)
