// Add JavaScript functionality for settings page

// Delete user function
function deleteUser() {
    // Send DELETE request to /auth/user endpoint
    fetch("/auth/user", {
        method: "DELETE",
        headers: {
            "Authorization": localStorage.getItem("JWT")
        }
    })
        .then(response => response.json())
        .then(data => console.log(data))
        .catch(error => console.error(error));
}

// Add radio function
function addRadio() {
    // Send POST request to /radios endpoint
    fetch("/" + document.getElementById("radio-id").value, {
        method: "PUT",
        headers: {
            "Content-Type": "application/json",
            "Authorization": localStorage.getItem("JWT")
        },
        body: JSON.stringify({
            title: document.getElementById("radio-title").value,
            description: document.getElementById("radio-description").value
        })
    })
        .then(response => response.json())
        .then(data => console.log(data))
        .catch(error => console.error(error));
}


// settings.js

const songQueue = [];

function addSong() {
    const fileInput = document.getElementById('music-file');
    const file = fileInput.files[0];

    if (file) {
        const songTitle = file.name; // Use the file name as the song title
        songQueue.push({ title: songTitle, file: file });
        updateSongQueue();
        fileInput.value = ''; // Clear the file input
    } else {
        alert('Please select a music file.');
    }
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
        row.insertCell(0).innerText = song.title;
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