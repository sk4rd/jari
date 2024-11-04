
const rx = /([^\/]*)\/edit/g;
const id = rx.exec(document.URL)[1];
let file_upload = document.getElementById("upload");
file_upload.addEventListener("change", () => {
    let file = file_upload.files[0];
    let form = new FormData();
    form.append("file", file);
    fetch("/" + id + "/songs/" + file.name, {method: "PUT", body: form, headers: {"Authorization": localStorage.getItem("JWT")}})
})


function get_edit_content(){
    let title = document.getElementById("title").value;
    console.log(title);
    let description = document.getElementById("description").value;
    console.log(description)
    let rx = /([^\/]*)\/edit/g;
    let id = rx.exec(document.URL)[1];
    fetch("/" + id, {method:"POST", body: JSON.stringify({title, description}), headers: {"Content-Type":"application/json", "Authorization": localStorage.getItem("JWT")}})
}


var songQueue = [];
fetch("/" + id + "/order").then((res) => {
    res.json().then((res) => {
        songQueue = res
        updateSongQueue()
    })
})


function getSongs() {
    fetch("/" + id + "/songs").then((res) => res.json().then((res) => {
        let selections = document.getElementById("selections");
        selections.innerHTML = "";
        for (let i = 0; i < res.length; i++) {
            let option = new Option();
            option.value = res[i];
            option.innerText = res[i];
            selections.add(option);
        }
    }))
}
getSongs()

function deleteSong() {
    let selections = document.getElementById("selections");
    let song = selections.selectedOptions[0].value;
    fetch("/" + id + "/songs/" + song, {method: "DELETE", headers: {"Authorization": localStorage.getItem("JWT")}}).then(getSongs)
}

function addSong() {
    let selections = document.getElementById("selections");
    songQueue.push(selections.selectedOptions[0].value);
    updateSongQueue();
}

function removeSong(index) {
    songQueue.splice(index, 1);
    updateSongQueue();
}

function moveSongUp(index) {
    if (index > 0) {
        const song = songQueue.splice(index, 1)[0];
        songQueue.splice(index - 1, 0, song);
        updateSongQueue();
    }
}

function moveSongDown(index) {
    if (index < songQueue.length - 1) {
        const song = songQueue.splice(index, 1)[0];
        songQueue.splice(index + 1, 0, song);
        updateSongQueue();
    }
}

function updateSongQueue() {
    const tbody = document.getElementById('song-queue').getElementsByTagName('tbody')[0];
    tbody.innerHTML = ''; // Clear existing rows

    songQueue.forEach((song, index) => {
        const row = tbody.insertRow();
        row.insertCell(0).innerText = song;
        const actionCell = row.insertCell(1);
        const removeButton = document.createElement('button');
        removeButton.innerText = 'Remove';
        removeButton.onclick = () => removeSong(index);
        actionCell.appendChild(removeButton);

        const moveUpButton = document.createElement('button');
        moveUpButton.innerText = 'Move Up';
        moveUpButton.onclick = () => moveSongUp(index);
        actionCell.appendChild(moveUpButton);

        const moveDownButton = document.createElement('button');
        moveDownButton.innerText = 'Move Down';
        moveDownButton.onclick = () => moveSongDown(index);
        actionCell.appendChild(moveDownButton);
    });
}

function setOrder() {
    const tbody = document.getElementById('song-queue').getElementsByTagName('tbody')[0];

    let req = [];
    for (let i = 0; i < tbody.rows.length; i++) {
        req.push(tbody.rows[i].children[0].innerText)
    }
    fetch("/" + id + "/order", {method: "PUT", body: JSON.stringify(req), headers: {"Content-Type": "application/json", "Authorization": localStorage.getItem("JWT")}})
}
