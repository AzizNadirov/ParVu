const { ipcRenderer } = require('electron');
const duckdb = require('duckdb');
const fs = require('fs');

document.getElementById('loadDataButton').addEventListener('click', () => {
    const fileInput = document.getElementById('fileInput');
    const limitInput = document.getElementById('limitInput');
    const limit = limitInput.value || 100;

    if (fileInput.files.length > 0) {
        const file = fileInput.files[0];
        loadParquetFile(file, limit);
    } else {
        alert("Please select a file first.");
    }
});

ipcRenderer.on('open-file', (event, filePath) => {
    loadParquetFileFromPath(filePath, 100);
});

function loadParquetFile(file, limit) {
    const reader = new FileReader();
    reader.onload = async (e) => {
        try {
            const arrayBuffer = e.target.result;
            const buffer = Buffer.from(arrayBuffer);

            // Create an in-memory DuckDB database
            const db = new duckdb.Database(':memory:');
            const con = db.connect();

            // Write buffer to a temporary file
            const tempFilePath = 'temp_parquet_file.parquet';
            fs.writeFileSync(tempFilePath, buffer);

            // Query the Parquet file
            con.all(`CREATE TABLE parquet_data AS SELECT * FROM parquet_scan('${tempFilePath}')`, (err) => {
                if (err) {
                    console.error("Error creating table from Parquet file:", err);
                    document.getElementById('output').innerText = "Error creating table from Parquet file: " + err.message;
                } else {
                    con.all(`SELECT * FROM parquet_data LIMIT ${limit}`, (err, res) => {
                        if (err) {
                            console.error("Error querying Parquet table:", err);
                            document.getElementById('output').innerText = "Error querying Parquet table: " + err.message;
                        } else {
                            console.log("Query result:", res);

                            // Convert BigInt to string for serialization
                            const resultWithStringBigInt = res.map(row => {
                                for (let key in row) {
                                    if (typeof row[key] === 'bigint') {
                                        row[key] = row[key].toString();
                                    }
                                }
                                return row;
                            });

                            // Render the result as an HTML table
                            renderTable(resultWithStringBigInt);
                        }
                    });
                }
            });
        } catch (error) {
            console.error("Error reading the file:", error);
            document.getElementById('output').innerText = "Error reading the file: " + error.message;
        }
    };
    reader.readAsArrayBuffer(file);
}

function loadParquetFileFromPath(filePath, limit) {
    fs.readFile(filePath, (err, buffer) => {
        if (err) {
            console.error("Error reading the file:", err);
            document.getElementById('output').innerText = "Error reading the file: " + err.message;
            return;
        }

        try {
            // Create an in-memory DuckDB database
            const db = new duckdb.Database(':memory:');
            const con = db.connect();

            // Write buffer to a temporary file
            const tempFilePath = 'temp_parquet_file.parquet';
            fs.writeFileSync(tempFilePath, buffer);

            // Query the Parquet file
            con.all(`CREATE TABLE parquet_data AS SELECT * FROM parquet_scan('${tempFilePath}')`, (err) => {
                if (err) {
                    console.error("Error creating table from Parquet file:", err);
                    document.getElementById('output').innerText = "Error creating table from Parquet file: " + err.message;
                } else {
                    con.all(`SELECT * FROM parquet_data LIMIT ${limit}`, (err, res) => {
                        if (err) {
                            console.error("Error querying Parquet table:", err);
                            document.getElementById('output').innerText = "Error querying Parquet table: " + err.message;
                        } else {
                            console.log("Query result:", res);

                            // Convert BigInt to string for serialization
                            const resultWithStringBigInt = res.map(row => {
                                for (let key in row) {
                                    if (typeof row[key] === 'bigint') {
                                        row[key] = row[key].toString();
                                    }
                                }
                                return row;
                            });

                            // Render the result as an HTML table
                            renderTable(resultWithStringBigInt);
                        }
                    });
                }
            });
        } catch (error) {
            console.error("Error reading the file:", error);
            document.getElementById('output').innerText = "Error reading the file: " + error.message;
        }
    });
}

function renderTable(data) {
    if (data.length === 0) {
        document.getElementById('output').innerText = "No data available.";
        return;
    }

    const table = document.createElement('table');

    // Create table header
    const thead = table.createTHead();
    const headerRow = thead.insertRow();
    Object.keys(data[0]).forEach(key => {
        const th = document.createElement('th');
        th.appendChild(document.createTextNode(key));
        headerRow.appendChild(th);
    });

    // Create table body
    const tbody = table.createTBody();
    data.forEach(row => {
        const tr = tbody.insertRow();
        Object.values(row).forEach(value => {
            const td = tr.insertCell();
            td.appendChild(document.createTextNode(value));
        });
    });

    const outputDiv = document.getElementById('output');
    outputDiv.innerHTML = ""; // Clear previous output
    outputDiv.appendChild(table);
}
