import sys
from pathlib import Path
from typing import Union

from PyQt5.QtWidgets import (QApplication, QWidget, QVBoxLayout, QLabel, QLineEdit, QPushButton, QTextEdit, QFileDialog,
                             QTableWidget, QTableWidgetItem, QHBoxLayout, QMenu, QAction, QToolButton, QMainWindow, QMessageBox, QFormLayout, QDialog)
from PyQt5.QtGui import QSyntaxHighlighter, QTextCharFormat, QColor, QFont, QMovie
from PyQt5.QtCore import Qt, QThread, pyqtSignal, QRegExp, QPoint
import duckdb
import pandas as pd

from schemas import settings, Settings, recents
from query_revisor import Revisor, BadQueryException


class SQLHighlighter(QSyntaxHighlighter):
    def __init__(self, parent=None):
        super(SQLHighlighter, self).__init__(parent)
        self._highlighting_rules = []

        keyword_format = QTextCharFormat()
        keyword_format.setForeground(QColor("blue"))
        keyword_format.setFontWeight(QFont.Bold)
        keywords = settings.sql_keywords + [settings.render_vars(settings.default_data_var_name)]

        for keyword in keywords:
            pattern = QRegExp(f"\\b{keyword}\\b", Qt.CaseInsensitive)
            self._highlighting_rules.append((pattern, keyword_format))

    def highlightBlock(self, text):
        for pattern, format in self._highlighting_rules:
            index = pattern.indexIn(text)
            while index >= 0:
                length = pattern.matchedLength()
                self.setFormat(index, length, format)
                index = pattern.indexIn(text, index + length)
        self.setCurrentBlockState(0)

class QueryThread(QThread):
    resultReady = pyqtSignal(pd.DataFrame)
    errorOccurred = pyqtSignal(str)

    def __init__(self, file_path, query, offset, limit, app: 'ParquetSQLApp'):
        super().__init__()
        self.file_path = file_path
        self.query = query
        self.offset = offset
        self.limit = limit
        self.app = app

    def queryRevisor(self, query: str) -> Union[str, None]:
        """ do checking and changes in query before it goes to run """
        rev_res = Revisor(query).run()
        if rev_res is True:
            return query
        
        elif isinstance(rev_res, BadQueryException):
            return rev_res
        
    def run(self):
        try:
            con = duckdb.connect(database=':memory:')
            creator_query = f"CREATE TABLE {settings.render_vars(settings.default_data_var_name)} AS SELECT * FROM '{self.file_path}'"
            con.execute(creator_query)
            paginated_query = self.query
            if "LIMIT" not in paginated_query.upper():
                paginated_query = f"{paginated_query} LIMIT {self.limit} OFFSET {self.offset}"
            paginated_query = self.queryRevisor(paginated_query)

            if isinstance(paginated_query, BadQueryException):
                # if revisor fails our query then throw the exception
                raise Exception(paginated_query.name + ": " + paginated_query.message)
            
            df = con.execute(paginated_query).fetchdf()
            self.resultReady.emit(df)
            
        except Exception as e:
            err_message = f"""
                            An error occurred while executing the query: '{creator_query}'\n
                            Error: '{str(e)}'
                        """
            self.errorOccurred.emit(err_message)


class ParquetSQLApp(QMainWindow):
    def __init__(self, file_path=None):
        super().__init__()
        self.setWindowTitle('Parquet SQL Executor')

        self.page = 0
        self.rows_per_page = settings.render_vars(settings.result_pagination_rows_per_page)
        self.active_filters = {}
        self.df = pd.DataFrame()

        self.initUI()

        if file_path:
            self.filePathEdit.setText(file_path)
            self.executeQuery()

    def initUI(self):
        self.showMaximized()  # Start the main window in full-screen mode
        layout = QVBoxLayout()

        self.fileLabel = QLabel('Parquet File Path:')
        layout.addWidget(self.fileLabel)

        self.filePathEdit = QLineEdit()
        layout.addWidget(self.filePathEdit)

        self.browseButton = QPushButton('Browse')
        self.browseButton.clicked.connect(self.browseFile)
        layout.addWidget(self.browseButton)

        self.sqlLabel = QLabel(f'Data Query - AS {settings.render_vars(settings.default_data_var_name)}:')
        self.sqlLabel.setFont(QFont("Courier", 8))
        layout.addWidget(self.sqlLabel)

        self.sqlEdit = QTextEdit()
        self.sqlEdit.setPlainText(settings.render_vars(settings.default_sql_query))
        layout.addWidget(self.sqlEdit)

        self.executeButton = QPushButton('Execute')
        self.executeButton.clicked.connect(self.executeQuery)
        layout.addWidget(self.executeButton)

        self.filterButton = QPushButton('Filter')
        self.filterButton.clicked.connect(self.toggleFilterState)
        layout.addWidget(self.filterButton)

        self.filtersMenuButton = QToolButton()
        self.filtersMenuButton.setText('Applied Filters')
        self.filtersMenuButton.setMinimumSize(150, 30)
        self.filtersMenuButton.setPopupMode(QToolButton.InstantPopup)
        self.filtersMenu = QMenu(self.filtersMenuButton)
        self.filtersMenuButton.setMenu(self.filtersMenu)
        layout.addWidget(self.filtersMenuButton)

        self.resultLabel = QLabel('Results:')
        layout.addWidget(self.resultLabel)

        self.resultTable = QTableWidget()
        self.resultTable.setContextMenuPolicy(Qt.CustomContextMenu)
        self.resultTable.customContextMenuRequested.connect(self.showContextMenu)
        layout.addWidget(self.resultTable)

        self.paginationLayout = QHBoxLayout()
        self.prevButton = QPushButton('Previous')
        self.prevButton.clicked.connect(self.prevPage)
        self.paginationLayout.addWidget(self.prevButton)

        self.nextButton = QPushButton('Next')
        self.nextButton.clicked.connect(self.nextPage)
        self.paginationLayout.addWidget(self.nextButton)

        layout.addLayout(self.paginationLayout)

        self.loadingLabel = QLabel()
        layout.addWidget(self.loadingLabel)
        self.loadingLabel.setVisible(False)

        central_widget = QWidget()
        central_widget.setLayout(layout)
        self.setCentralWidget(central_widget)

        # Set up syntax highlighting
        self.setupSqlEdit()

        # Create menu bar
        self.createMenuBar()

    def setupSqlEdit(self):
        # Set monospace font for consistency
        font = self.sqlEdit.font()
        font.setFamily(settings.default_sql_font)
        font.setPointSize(int(settings.default_sql_font_size))
        self.sqlEdit.setFont(font)

        # Apply syntax highlighting
        self.highlighter = SQLHighlighter(self.sqlEdit.document())

    def createMenuBar(self):
        menubar = self.menuBar()
        fileMenu = menubar.addMenu('File')

        # export
        exportAction = QAction('Export', self)
        exportAction.triggered.connect(self.exportResults)
        fileMenu.addAction(exportAction)

        # settings
        settingsAction = QAction('Settings', self)
        settingsAction.triggered.connect(self.editSettings)
        fileMenu.addAction(settingsAction)

        # recents
        recentsMenu = fileMenu.addMenu('Recents')
        self.recentsMenu = recentsMenu
        self.updateRecentsMenu()

    def browseFile(self):
        options = QFileDialog.Options()
        fileName, _ = QFileDialog.getOpenFileName(self, "Open Parquet File", "", "Parquet Files (*.parquet);;All Files (*)", options=options)
        if fileName:
            self.filePathEdit.setText(fileName)
            # if tracking history is enabled
            if settings.save_file_history in ("True", "true", "1", True, 1):
                recents.add_recent(fileName)

            self.updateRecentsMenu()

    def executeQuery(self):
        self.page = 0
        self.active_filters = {}
        self.updateFiltersMenu()
        self.loadPage()

    def loadPage(self):
        file_path = self.filePathEdit.text()
        query = self.sqlEdit.toPlainText()

        if file_path and query:
            self.showLoadingAnimation()
            self.thread = QueryThread(file_path, query, self.page * self.rows_per_page, self.rows_per_page, self)
            self.thread.resultReady.connect(self.handleResults)
            self.thread.errorOccurred.connect(self.handleError)
            self.thread.start()
        else:
            self.resultLabel.setText("Please provide both file path and SQL query.")

    def handleResults(self, df):
        self.thread.quit()
        self.thread.wait()
        self.hideLoadingAnimation()
        self.df = df
        self.applyFilters()

    def handleError(self, error):
        self.thread.quit()
        self.thread.wait()
        self.hideLoadingAnimation()
        self.resultLabel.setText(f"Error: {error}")

    def showLoadingAnimation(self):
        self.loadingLabel.setVisible(True)
        self.movie = QMovie("./static/loading-thinking.gif")
        self.loadingLabel.setMovie(self.movie)
        self.movie.start()

    def hideLoadingAnimation(self):
        self.movie.stop()
        self.loadingLabel.setVisible(False)

    def displayResults(self, df):
        # Set the table dimensions
        self.resultTable.setColumnCount(len(df.columns))
        self.resultTable.setRowCount(len(df.index))

        # Set the column headers
        self.resultTable.setHorizontalHeaderLabels(df.columns)

        # Fill the table with the DataFrame data
        for i in range(len(df.index)):
            for j in range(len(df.columns)):
                self.resultTable.setItem(i, j, QTableWidgetItem(str(df.iat[i, j])))

        # Resize columns to fit content
        self.resultTable.resizeColumnsToContents()

    def prevPage(self):
        if self.page > 0:
            self.page -= 1
            self.loadPage()

    def nextPage(self):
        self.page += 1
        self.loadPage()

    def showContextMenu(self, pos):
        contextMenu = QMenu(self)

        header = self.resultTable.horizontalHeader()
        column = header.logicalIndexAt(pos.x())
        row = self.resultTable.indexAt(pos).row()

        if column >= 0:
            column_name = self.resultTable.horizontalHeaderItem(column).text()

            # Create Copy Submenu
            copy_menu = QMenu("Copy", self)
            copy_column_action = QAction("Copy Column Name", self)
            copy_column_action.triggered.connect(lambda: self.copyColumnName(column_name))
            copy_menu.addAction(copy_column_action)

            copy_column_values_action = QAction("Copy Whole Column", self)
            copy_column_values_action.triggered.connect(lambda: self.copyColumnValues(column))
            copy_menu.addAction(copy_column_values_action)

            if row >= 0:
                copy_row_values_action = QAction("Copy Whole Row", self)
                copy_row_values_action.triggered.connect(lambda: self.copyRowValues(row))
                copy_menu.addAction(copy_row_values_action)

            contextMenu.addMenu(copy_menu)

            filter_action = QAction("Filter for value", self)
            filter_action.triggered.connect(lambda: self.showFilterMenu(column))
            contextMenu.addAction(filter_action)

        contextMenu.exec_(self.resultTable.mapToGlobal(pos))

    def copyColumnName(self, column_name):
        clipboard = QApplication.clipboard()
        clipboard.setText(column_name)

    def copyColumnValues(self, column):
        values = self.df.iloc[:, column].tolist()
        clipboard = QApplication.clipboard()
        clipboard.setText(str(values))

    def copyRowValues(self, row):
        values = self.df.iloc[row, :].tolist()
        clipboard = QApplication.clipboard()
        clipboard.setText(str(values))

    def toggleFilterState(self):
        if self.filterButton.text() == 'Filter':
            self.activateFiltering()
        else:
            self.resetFilters()

    def activateFiltering(self):
        self.filterButton.setText('Clear Filters')
        self.active_filters = {}
        self.updateFiltersMenu()
        self.loadPage()

    def resetFilters(self):
        self.filterButton.setText('Filter')
        self.active_filters = {}
        self.updateFiltersMenu()
        self.applyFilters()

    def showFilterMenu(self, column):
        if self.df is None or self.df.empty:
            return

        unique_values = self.df.iloc[:, column].unique()
        filter_menu = QMenu(self)
        for value in unique_values:
            filter_action = QAction(str(value), self)
            filter_action.setCheckable(True)
            filter_action.setChecked(column in self.active_filters and value in self.active_filters[column])
            filter_action.triggered.connect(lambda checked, val=value: self.toggleFilter(column, val, checked))
            filter_menu.addAction(filter_action)

        header_pos = self.resultTable.horizontalHeader().sectionPosition(column)
        header_pos_global = self.resultTable.horizontalHeader().mapToGlobal(QPoint(header_pos, 0))
        filter_menu.exec_(header_pos_global)

    def toggleFilter(self, column, value, checked):
        if checked:
            if column not in self.active_filters:
                self.active_filters[column] = set()
            self.active_filters[column].add(value)
        else:
            if column in self.active_filters and value in self.active_filters[column]:
                self.active_filters[column].remove(value)
                if not self.active_filters[column]:
                    del self.active_filters[column]

        self.applyFilters()
        self.updateFiltersMenu()

    def applyFilters(self):
        if not self.active_filters:
            filtered_df = self.df
        else:
            filtered_df = self.df.copy()
            for column, values in self.active_filters.items():
                filtered_df = filtered_df[filtered_df.iloc[:, column].isin(values)]

        self.displayResults(filtered_df)


    def updateFiltersMenu(self):
        self.filtersMenu.clear()
        if not self.active_filters:
            no_filters_action = QAction('No Filters Applied', self)
            no_filters_action.setEnabled(False)
            self.filtersMenu.addAction(no_filters_action)
        else:
            for column, values in self.active_filters.items():
                for value in values:
                    filter_action = QAction(f"{self.resultTable.horizontalHeaderItem(column).text()}: {value}", self)
                    filter_action.setCheckable(True)
                    filter_action.setChecked(True)
                    filter_action.triggered.connect(lambda checked, col=column, val=value: self.toggleFilter(col, val, checked))
                    self.filtersMenu.addAction(filter_action)

    def exportResults(self):
        filtered_df = self.df.copy()
        for column, values in self.active_filters.items():
            filtered_df = filtered_df[filtered_df.iloc[:, column].isin(values)]

        if filtered_df.empty:
            QMessageBox.warning(self, "No Data", "There is no data to export.")
            return

        options = QFileDialog.Options()
        filePath, _ = QFileDialog.getSaveFileName(self, "Export Results", "", "CSV Files (*.csv);;Excel Files (*.xlsx);;All Files (*)", options=options)
        if filePath:
            if filePath.endswith('.csv'):
                filtered_df.to_csv(filePath, index=False)
            elif filePath.endswith('.xlsx'):
                filtered_df.to_excel(filePath, index=False)
            else:
                QMessageBox.warning(self, "Invalid File Type", "Please select a valid file type (CSV or XLSX).")

    def editSettings(self):
        settings_file = settings.settings_file
        default_settings_file = settings.default_settings_file
        if not Path(settings_file).exists():
            QMessageBox.critical(self, "Error", f"Settings file '{settings_file}' does not exist.")
            return

        class SettingsDialog(QDialog):
            read_only_fields = ["recents_file", 'settings_file', 'default_settings_file',]
            help_text = "Did you know:\nYou can use field names inside string as `$(field_name)` for render it."
            def __init__(self, settings: Settings, 
                         default_settings_file: Path):
                super().__init__()
                self.settings = settings
                self.default_settings_file = default_settings_file
                self.initUI()

            def validateSettings(self):
                for field, line_edit in self.fields.items():
                    if field in self.read_only_fields:
                        continue

                    if field == 'default_data_var_name':
                        if line_edit.text().upper() in settings.sql_keywords:
                            QMessageBox.critical(self, "Error", "The data variable name cannot be a SQL keyword.")
                            return False
                    if field == 'result_pagination_rows_per_page':
                        if not line_edit.text().isdigit() or int(line_edit.text()) < 1:
                            QMessageBox.critical(self, "Error", "The result pagination rows per page must be a positive integer.")
                            return False
                        if not (10 <= int(line_edit.text()) <= 1000):
                            QMessageBox.critical(self, "Error", "The result pagination rows per page must be between 10 and 1000.")
                            return False

                return True


            def initUI(self):
                layout = QFormLayout()

                self.fields = {}
                for field, value in self.settings.model_dump().items():
                    if field in self.read_only_fields:
                        continue

                    line_edit = QLineEdit()
                    # line_edit.setPlaceholderText(str(value))
                    line_edit.setText(str(value))
                    self.fields[field] = line_edit
                    layout.addRow(QLabel(field), line_edit)

                help_text = QLabel(self.help_text)
                help_text.setFont(QFont("Courier", 9, weight=QFont.Bold))
                layout.addRow(help_text)

                button_layout = QHBoxLayout()

                save_button = QPushButton("Save")
                save_button.clicked.connect(self.saveSettings)
                button_layout.addWidget(save_button)

                reset_button = QPushButton("Reset to Default")
                reset_button.clicked.connect(self.resetSettings)
                button_layout.addWidget(reset_button)

                layout.addRow(button_layout)

                self.setLayout(layout)
                self.setWindowTitle("Edit Settings")
                self.resize(400, 300)

            def saveSettings(self):
                if not self.validateSettings():
                    QMessageBox.critical(self, "Error", "Please fix the errors before saving.")
                    return
                for field, line_edit in self.fields.items():
                    if line_edit.text():
                        if field == 'sql_keywords':
                            # replace stringed list into list[str]
                            kws = line_edit.text()
                            kws = [i.strip().replace("'", "") for i in kws[1:-1].split(',')]
                            setattr(self.settings, field, kws)
                        else:
                            setattr(self.settings, field, line_edit.text())

                self.settings.save_settings()
                self.accept()

            def resetSettings(self):
                with self.default_settings_file.open("r") as f:
                    default_settings_data = f.read()
                with settings_file.open("w") as f:
                    f.write(default_settings_data)
                self.settings = Settings.load_settings()
                QMessageBox.information(self, "Settings Reset", "Settings have been reset to default values. Please restart the application for changes to take effect.")
                self.accept()

        dialog = SettingsDialog(settings, default_settings_file)
        dialog.exec_()


    def updateRecentsMenu(self):
        self.recentsMenu.clear()
        for recent in recents.recents:
            recent_action = QAction(recent, self)
            recent_action.triggered.connect(lambda checked, path=recent: self.openRecentFile(path))
            self.recentsMenu.addAction(recent_action)
        
        # Add Clear List action at the end
        if recents.recents:
            clear_action = QAction('Clear List', self)
            clear_action.setFont(QFont("Courier", 9, weight=QFont.Bold))
            clear_action.triggered.connect(self.clearRecents)
            self.recentsMenu.addAction(clear_action)

    def clearRecents(self):
        recents.recents = []
        recents.save_recents()
        self.updateRecentsMenu()

    def openRecentFile(self, file_path):
        if not Path(file_path).exists():
            reply = QMessageBox.question(self, 'File Not Found',
                                         f"The file {file_path} does not exist. Do you want to remove it from recents?",
                                         QMessageBox.Yes | QMessageBox.No, QMessageBox.No)
            if reply == QMessageBox.Yes:
                recents.recents.remove(file_path)
                recents.save_recents()
                self.updateRecentsMenu()
            return

        self.filePathEdit.setText(file_path)
        self.executeQuery()


if __name__ == '__main__':
    app = QApplication(sys.argv)
    file_path = sys.argv[1] if len(sys.argv) > 1 else None
    ex = ParquetSQLApp(file_path)
    ex.show()
    sys.exit(app.exec_())
