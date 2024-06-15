import sys
from PyQt5.QtWidgets import (QApplication, QWidget, QVBoxLayout, QLabel, QLineEdit, QPushButton, QTextEdit, QFileDialog, 
                             QTableWidget, QTableWidgetItem, QHBoxLayout, QMenu, QAction)
from PyQt5.QtGui import QSyntaxHighlighter, QTextCharFormat, QColor, QFont, QMovie
from PyQt5.QtCore import Qt, QThread, pyqtSignal, QRegExp
import duckdb
import pandas as pd

class SQLHighlighter(QSyntaxHighlighter):
    def __init__(self, parent=None):
        super(SQLHighlighter, self).__init__(parent)
        self._highlighting_rules = []

        keyword_format = QTextCharFormat()
        keyword_format.setForeground(QColor("blue"))
        keyword_format.setFontWeight(QFont.Bold)
        keywords = [
            "SELECT", "FROM", "WHERE", "GROUP BY", "ORDER BY", 
            "JOIN", "LEFT JOIN", "RIGHT JOIN", "INNER JOIN", "ON", 
            "AS", "DISTINCT", "AND", "OR", "NOT", "IN", 
            "LIKE", "NULL", "IS", "BETWEEN", "HAVING", "CASE", 
            "WHEN", "THEN", "ELSE", "END", "LIMIT", "DATA"
        ]

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

    def __init__(self, file_path, query, offset, limit):
        super().__init__()
        self.file_path = file_path
        self.query = query
        self.offset = offset
        self.limit = limit

    def run(self):
        try:
            con = duckdb.connect(database=':memory:')
            con.execute(f"CREATE TABLE DATA AS SELECT * FROM '{self.file_path}'")
            paginated_query = f"{self.query} LIMIT {self.limit} OFFSET {self.offset}"
            df = con.execute(paginated_query).fetchdf()
            self.resultReady.emit(df)
        except Exception as e:
            self.errorOccurred.emit(str(e))

class ParquetSQLApp(QWidget):
    def __init__(self):
        super().__init__()

        self.page = 0
        self.rows_per_page = 1000

        self.initUI()

    def initUI(self):
        self.setWindowTitle('Parquet SQL Executor')

        layout = QVBoxLayout()

        self.fileLabel = QLabel('Parquet File Path:')
        layout.addWidget(self.fileLabel)

        self.filePathEdit = QLineEdit()
        layout.addWidget(self.filePathEdit)

        self.browseButton = QPushButton('Browse')
        self.browseButton.clicked.connect(self.browseFile)
        layout.addWidget(self.browseButton)

        self.sqlLabel = QLabel('SQL Query:')
        layout.addWidget(self.sqlLabel)

        self.sqlEdit = QTextEdit()
        self.sqlEdit.setPlainText("SELECT * FROM DATA LIMIT 100")
        layout.addWidget(self.sqlEdit)

        self.executeButton = QPushButton('Execute')
        self.executeButton.clicked.connect(self.executeQuery)
        layout.addWidget(self.executeButton)

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

        self.setLayout(layout)

        # Set up syntax highlighting
        self.setupSqlEdit()

    def setupSqlEdit(self):
        # Set monospace font for consistency
        font = self.sqlEdit.font()
        font.setFamily("Courier")
        self.sqlEdit.setFont(font)

        # Apply syntax highlighting
        self.highlighter = SQLHighlighter(self.sqlEdit.document())

    def browseFile(self):
        options = QFileDialog.Options()
        fileName, _ = QFileDialog.getOpenFileName(self, "Open Parquet File", "", "Parquet Files (*.parquet);;All Files (*)", options=options)
        if fileName:
            self.filePathEdit.setText(fileName)

    def executeQuery(self):
        self.page = 0
        self.loadPage()

    def loadPage(self):
        file_path = self.filePathEdit.text()
        query = self.sqlEdit.toPlainText()

        if file_path and query:
            self.showLoadingAnimation()
            self.thread = QueryThread(file_path, query, self.page * self.rows_per_page, self.rows_per_page)
            self.thread.resultReady.connect(self.handleResults)
            self.thread.errorOccurred.connect(self.handleError)
            self.thread.start()
        else:
            self.resultLabel.setText("Please provide both file path and SQL query.")

    def handleResults(self, df):
        self.thread.quit()
        self.thread.wait()
        self.hideLoadingAnimation()
        self.displayResults(df)

    def handleError(self, error):
        self.thread.quit()
        self.thread.wait()
        self.hideLoadingAnimation()
        self.resultLabel.setText(f"Error: {error}")

    def showLoadingAnimation(self):
        self.loadingLabel.setVisible(True)
        self.movie = QMovie("loading.gif")
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
        if column >= 0:
            column_name = self.resultTable.horizontalHeaderItem(column).text()
            copy_action = QAction(f"Copy column name: {column_name}", self)
            copy_action.triggered.connect(lambda: self.copyColumnName(column_name))
            contextMenu.addAction(copy_action)

        contextMenu.exec_(self.resultTable.mapToGlobal(pos))

    def copyColumnName(self, column_name):
        clipboard = QApplication.clipboard()
        clipboard.setText(column_name)

if __name__ == '__main__':
    app = QApplication(sys.argv)
    ex = ParquetSQLApp()
    ex.show()
    sys.exit(app.exec_())
