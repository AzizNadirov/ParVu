const { app, BrowserWindow, dialog } = require('electron');
const path = require('path');
const fs = require('fs');

let mainWindow;

function createWindow() {
    mainWindow = new BrowserWindow({
        width: 800,
        height: 600,
        webPreferences: {
            preload: path.join(__dirname, 'renderer.js'),
            nodeIntegration: true,
            contextIsolation: false
        }
    });

    mainWindow.loadFile('index.html');

    // Handle open file event
    app.on('open-file', (event, filePath) => {
        event.preventDefault();
        openParquetFile(filePath);
    });

    if (process.argv.length >= 2) {
        openParquetFile(process.argv[1]);
    }
}

function openParquetFile(filePath) {
    if (mainWindow) {
        mainWindow.webContents.send('open-file', filePath);
    }
}

app.whenReady().then(() => {
    createWindow();

    app.on('activate', () => {
        if (BrowserWindow.getAllWindows().length === 0) {
            createWindow();
        }
    });
});

app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
        app.quit();
    }
});

app.on('second-instance', (event, commandLine, workingDirectory) => {
    if (process.platform !== 'darwin') {
        openParquetFile(commandLine[1]);
    }
});

if (!app.requestSingleInstanceLock()) {
    app.quit();
} else {
    app.on('second-instance', (event, commandLine, workingDirectory) => {
        if (mainWindow) {
            if (mainWindow.isMinimized()) mainWindow.restore();
            mainWindow.focus();
            openParquetFile(commandLine[1]);
        }
    });
}

app.setAsDefaultProtocolClient('parquet');
