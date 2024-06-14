import sys
from PyQt5.QtWidgets import QApplication, QWidget, QVBoxLayout, QLabel, QLineEdit, QPushButton, QTextEdit, QFileDialog, QTableWidget, QTableWidgetItem, QMenu, QAction
from PyQt5.QtCore import Qt
import duckdb
import pandas as pd

class ParquetSQLApp(QWidget):
    def __init__(self):
        super().__init__()

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
        self.sqlEdit.setPlainText("SELECT * FROM DATA")
        layout.addWidget(self.sqlEdit)

        self.executeButton = QPushButton('Execute')
        self.executeButton.clicked.connect(self.executeQuery)
        layout.addWidget(self.executeButton)

        self.resultLabel = QLabel('Results:')
        layout.addWidget(self.resultLabel)

        self.resultTable = QTableWidget()
        layout.addWidget(self.resultTable)

        self.setLayout(layout)

        # Set up context menu for column headers
        self.resultTable.horizontalHeader().setContextMenuPolicy(Qt.CustomContextMenu)
        self.resultTable.horizontalHeader().customContextMenuRequested.connect(self.showColumnContextMenu)

    def browseFile(self):
        options = QFileDialog.Options()
        fileName, _ = QFileDialog.getOpenFileName(self, "Open Parquet File", "", "Parquet Files (*.parquet);;All Files (*)", options=options)
        if fileName:
            self.filePathEdit.setText(fileName)

    def executeQuery(self):
        file_path = self.filePathEdit.text()
        query = self.sqlEdit.toPlainText()

        if file_path and query:
            try:
                # Execute SQL query on the Parquet file using DuckDB
                con = duckdb.connect(database=':memory:')
                con.execute(f"CREATE TABLE DATA AS SELECT * FROM '{file_path}'")
                df = con.execute(query).fetchdf()

                # Display the results in the table
                self.displayResults(df)

            except Exception as e:
                self.resultTable.setRowCount(0)
                self.resultTable.setColumnCount(0)
                self.resultLabel.setText(f"Error: {str(e)}")
        else:
            self.resultLabel.setText("Please provide both file path and SQL query.")

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

    def showColumnContextMenu(self, pos):
        column_index = self.resultTable.horizontalHeader().logicalIndexAt(pos.x())

        menu = QMenu(self)

        if column_index >= 0:
            column_name = self.resultTable.horizontalHeaderItem(column_index).text()

            action_copy_column_name = QAction(f"Copy '{column_name}' to Clipboard", self)
            action_copy_column_name.triggered.connect(lambda: self.copyColumnName(column_name))
            menu.addAction(action_copy_column_name)

        menu.exec_(self.resultTable.mapToGlobal(pos))

    def copyColumnName(self, column_name):
        clipboard = QApplication.clipboard()
        clipboard.setText(column_name)

if __name__ == '__main__':
    app = QApplication(sys.argv)
    ex = ParquetSQLApp()
    ex.show()
    sys.exit(app.exec_())
