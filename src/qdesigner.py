# -*- coding: utf-8 -*-

################################################################################
## Form generated from reading UI file 'ParVuGfSbKl.ui'
##
## Created by: Qt User Interface Compiler version 6.7.2
##
## WARNING! All changes made in this file will be lost when recompiling UI file!
################################################################################

from PySide6.QtCore import (QCoreApplication, QDate, QDateTime, QLocale,
    QMetaObject, QObject, QPoint, QRect,
    QSize, QTime, QUrl, Qt)
from PySide6.QtGui import (QAction, QBrush, QColor, QConicalGradient,
    QCursor, QFont, QFontDatabase, QGradient,
    QIcon, QImage, QKeySequence, QLinearGradient,
    QPainter, QPalette, QPixmap, QRadialGradient,
    QTransform)
from PySide6.QtWidgets import (QAbstractScrollArea, QApplication, QFrame, QHBoxLayout,
    QHeaderView, QMainWindow, QMenu, QMenuBar,
    QPushButton, QScrollArea, QSizePolicy, QSpacerItem,
    QStatusBar, QTableView, QTextBrowser, QTextEdit,
    QWidget)

class Ui_MainWindow(object):
    def setupUi(self, MainWindow):
        if not MainWindow.objectName():
            MainWindow.setObjectName(u"MainWindow")
        MainWindow.resize(1143, 1023)
        self.actionSave_as = QAction(MainWindow)
        self.actionSave_as.setObjectName(u"actionSave_as")
        self.actionOpen = QAction(MainWindow)
        self.actionOpen.setObjectName(u"actionOpen")
        self.actionPreferences = QAction(MainWindow)
        self.actionPreferences.setObjectName(u"actionPreferences")
        self.actionInfo = QAction(MainWindow)
        self.actionInfo.setObjectName(u"actionInfo")
        self.actionItem_1 = QAction(MainWindow)
        self.actionItem_1.setObjectName(u"actionItem_1")
        self.actionClear = QAction(MainWindow)
        self.actionClear.setObjectName(u"actionClear")
        self.centralwidget = QWidget(MainWindow)
        self.centralwidget.setObjectName(u"centralwidget")
        self.query_scroll = QScrollArea(self.centralwidget)
        self.query_scroll.setObjectName(u"query_scroll")
        self.query_scroll.setGeometry(QRect(20, 10, 1081, 61))
        self.query_scroll.setWidgetResizable(True)
        self.scrollAreaWidgetContents = QWidget()
        self.scrollAreaWidgetContents.setObjectName(u"scrollAreaWidgetContents")
        self.scrollAreaWidgetContents.setGeometry(QRect(0, 0, 1079, 59))
        self.query_editor = QTextEdit(self.scrollAreaWidgetContents)
        self.query_editor.setObjectName(u"query_editor")
        self.query_editor.setGeometry(QRect(0, 0, 1081, 61))
        font = QFont()
        font.setFamilies([u"Ubuntu Mono"])
        font.setPointSize(14)
        font.setBold(True)
        self.query_editor.setFont(font)
        self.query_editor.setAutoFillBackground(False)
        self.query_editor.setFrameShape(QFrame.Shape.Box)
        self.query_editor.setLineWidth(0)
        self.query_editor.setSizeAdjustPolicy(QAbstractScrollArea.SizeAdjustPolicy.AdjustToContents)
        self.query_editor.setAcceptRichText(False)
        self.query_scroll.setWidget(self.scrollAreaWidgetContents)
        self.table = QTableView(self.centralwidget)
        self.table.setObjectName(u"table")
        self.table.setGeometry(QRect(20, 170, 1081, 751))
        self.data_meta = QFrame(self.centralwidget)
        self.data_meta.setObjectName(u"data_meta")
        self.data_meta.setGeometry(QRect(270, 250, 631, 431))
        palette = QPalette()
        brush = QBrush(QColor(222, 221, 218, 255))
        brush.setStyle(Qt.SolidPattern)
        palette.setBrush(QPalette.Active, QPalette.Window, brush)
        palette.setBrush(QPalette.Inactive, QPalette.Window, brush)
        palette.setBrush(QPalette.Disabled, QPalette.Base, brush)
        palette.setBrush(QPalette.Disabled, QPalette.Window, brush)
        self.data_meta.setPalette(palette)
        font1 = QFont()
        font1.setPointSize(14)
        self.data_meta.setFont(font1)
        self.data_meta.setFrameShape(QFrame.Shape.StyledPanel)
        self.data_meta.setFrameShadow(QFrame.Shadow.Raised)
        self.popup_info = QTextBrowser(self.data_meta)
        self.popup_info.setObjectName(u"popup_info")
        self.popup_info.setGeometry(QRect(0, 0, 631, 431))
        self.popup_info.setFrameShape(QFrame.Shape.NoFrame)
        self.popup_info.setSizeAdjustPolicy(QAbstractScrollArea.SizeAdjustPolicy.AdjustToContents)
        self.popup_info.setAutoFormatting(QTextEdit.AutoFormattingFlag.AutoAll)
        self.popup_info.setTabChangesFocus(True)
        self.popup_info.setUndoRedoEnabled(False)
        self.popup_info.setAcceptRichText(False)
        self.popup_info.setOpenExternalLinks(True)
        self.horizontalLayoutWidget = QWidget(self.centralwidget)
        self.horizontalLayoutWidget.setObjectName(u"horizontalLayoutWidget")
        self.horizontalLayoutWidget.setGeometry(QRect(20, 80, 1081, 41))
        self.horizontalLayout = QHBoxLayout(self.horizontalLayoutWidget)
        self.horizontalLayout.setObjectName(u"horizontalLayout")
        self.horizontalLayout.setContentsMargins(0, 0, 0, 0)
        self.browse_file = QPushButton(self.horizontalLayoutWidget)
        self.browse_file.setObjectName(u"browse_file")
        palette1 = QPalette()
        brush1 = QBrush(QColor(143, 240, 164, 255))
        brush1.setStyle(Qt.SolidPattern)
        palette1.setBrush(QPalette.Active, QPalette.Button, brush1)
        palette1.setBrush(QPalette.Inactive, QPalette.Button, brush1)
        palette1.setBrush(QPalette.Disabled, QPalette.Button, brush1)
        self.browse_file.setPalette(palette1)

        self.horizontalLayout.addWidget(self.browse_file)

        self.horizontalLayoutWidget_2 = QWidget(self.centralwidget)
        self.horizontalLayoutWidget_2.setObjectName(u"horizontalLayoutWidget_2")
        self.horizontalLayoutWidget_2.setGeometry(QRect(20, 120, 1081, 41))
        self.horizontalLayout_2 = QHBoxLayout(self.horizontalLayoutWidget_2)
        self.horizontalLayout_2.setObjectName(u"horizontalLayout_2")
        self.horizontalLayout_2.setContentsMargins(0, 0, 0, 0)
        self.result_meta = QPushButton(self.horizontalLayoutWidget_2)
        self.result_meta.setObjectName(u"result_meta")
        palette2 = QPalette()
        brush2 = QBrush(QColor(46, 194, 126, 255))
        brush2.setStyle(Qt.SolidPattern)
        palette2.setBrush(QPalette.Active, QPalette.Button, brush2)
        palette2.setBrush(QPalette.Inactive, QPalette.Button, brush2)
        palette2.setBrush(QPalette.Disabled, QPalette.Button, brush2)
        self.result_meta.setPalette(palette2)

        self.horizontalLayout_2.addWidget(self.result_meta)

        self.result_meta_2 = QPushButton(self.horizontalLayoutWidget_2)
        self.result_meta_2.setObjectName(u"result_meta_2")
        palette3 = QPalette()
        brush3 = QBrush(QColor(153, 193, 241, 255))
        brush3.setStyle(Qt.SolidPattern)
        palette3.setBrush(QPalette.Active, QPalette.Button, brush3)
        palette3.setBrush(QPalette.Inactive, QPalette.Button, brush3)
        palette3.setBrush(QPalette.Disabled, QPalette.Button, brush3)
        self.result_meta_2.setPalette(palette3)

        self.horizontalLayout_2.addWidget(self.result_meta_2)

        self.horizontalLayoutWidget_3 = QWidget(self.centralwidget)
        self.horizontalLayoutWidget_3.setObjectName(u"horizontalLayoutWidget_3")
        self.horizontalLayoutWidget_3.setGeometry(QRect(20, 930, 1081, 51))
        self.horizontalLayout_3 = QHBoxLayout(self.horizontalLayoutWidget_3)
        self.horizontalLayout_3.setObjectName(u"horizontalLayout_3")
        self.horizontalLayout_3.setContentsMargins(0, 0, 0, 0)
        self.horizontalLayout_4 = QHBoxLayout()
        self.horizontalLayout_4.setObjectName(u"horizontalLayout_4")
        self.horizontalSpacer = QSpacerItem(40, 20, QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)

        self.horizontalLayout_4.addItem(self.horizontalSpacer)

        self.btn_pgn_next = QPushButton(self.horizontalLayoutWidget_3)
        self.btn_pgn_next.setObjectName(u"btn_pgn_next")

        self.horizontalLayout_4.addWidget(self.btn_pgn_next)

        self.btn_pgn_prev = QPushButton(self.horizontalLayoutWidget_3)
        self.btn_pgn_prev.setObjectName(u"btn_pgn_prev")

        self.horizontalLayout_4.addWidget(self.btn_pgn_prev)


        self.horizontalLayout_3.addLayout(self.horizontalLayout_4)

        MainWindow.setCentralWidget(self.centralwidget)
        self.menubar = QMenuBar(MainWindow)
        self.menubar.setObjectName(u"menubar")
        self.menubar.setGeometry(QRect(0, 0, 1143, 20))
        self.menuFile = QMenu(self.menubar)
        self.menuFile.setObjectName(u"menuFile")
        self.menuRecents = QMenu(self.menuFile)
        self.menuRecents.setObjectName(u"menuRecents")
        self.menuTools = QMenu(self.menubar)
        self.menuTools.setObjectName(u"menuTools")
        MainWindow.setMenuBar(self.menubar)
        self.statusbar = QStatusBar(MainWindow)
        self.statusbar.setObjectName(u"statusbar")
        MainWindow.setStatusBar(self.statusbar)

        self.menubar.addAction(self.menuFile.menuAction())
        self.menubar.addAction(self.menuTools.menuAction())
        self.menuFile.addAction(self.actionSave_as)
        self.menuFile.addSeparator()
        self.menuFile.addAction(self.menuRecents.menuAction())
        self.menuRecents.addAction(self.actionItem_1)
        self.menuRecents.addSeparator()
        self.menuRecents.addAction(self.actionClear)
        self.menuTools.addAction(self.actionPreferences)
        self.menuTools.addAction(self.actionInfo)

        self.retranslateUi(MainWindow)

        QMetaObject.connectSlotsByName(MainWindow)
    # setupUi

    def retranslateUi(self, MainWindow):
        MainWindow.setWindowTitle(QCoreApplication.translate("MainWindow", u"ParVu - Parquet Viewer", None))
        self.actionSave_as.setText(QCoreApplication.translate("MainWindow", u"Save as", None))
        self.actionOpen.setText(QCoreApplication.translate("MainWindow", u"Open", None))
        self.actionPreferences.setText(QCoreApplication.translate("MainWindow", u"Preferences", None))
        self.actionInfo.setText(QCoreApplication.translate("MainWindow", u"Info", None))
        self.actionItem_1.setText(QCoreApplication.translate("MainWindow", u"Item_1", None))
        self.actionClear.setText(QCoreApplication.translate("MainWindow", u"Clear", None))
        self.query_editor.setPlaceholderText("")
        self.popup_info.setDocumentTitle("")
        self.popup_info.setMarkdown(QCoreApplication.translate("MainWindow", u"# some txt\n"
"\n"
"", None))
        self.browse_file.setText(QCoreApplication.translate("MainWindow", u"Browse", None))
        self.result_meta.setText(QCoreApplication.translate("MainWindow", u"Execute", None))
        self.result_meta_2.setText(QCoreApplication.translate("MainWindow", u"Info", None))
        self.btn_pgn_next.setText(QCoreApplication.translate("MainWindow", u"Next", None))
        self.btn_pgn_prev.setText(QCoreApplication.translate("MainWindow", u"Prev", None))
        self.menuFile.setTitle(QCoreApplication.translate("MainWindow", u"File", None))
        self.menuRecents.setTitle(QCoreApplication.translate("MainWindow", u"Recents", None))
        self.menuTools.setTitle(QCoreApplication.translate("MainWindow", u"Tools", None))
    # retranslateUi

