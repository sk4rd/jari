// Get the play/pause button and the audio element
const playPauseButton = document.getElementById('playPauseButton');
const audio = document.getElementById('audio');

if (localStorage.getItem("JWT")) {
    document.getElementById("navbutton").onclick = () => window.location.href=window.location.href+"/edit";
    document.getElementById("navbutton").innerText="Edit Radio";
}

// Add an event listener to the play/pause button
playPauseButton.addEventListener('click', () => {
    // Check if the audio is playing
    if (audio.paused) {
        // Play the audio
        audio.play();
        // Update the play/pause button icon
        playPauseButton.querySelector('.play-pause-icon').classList.remove('fa-play');
        playPauseButton.querySelector('.play-pause-icon').classList.add('fa-pause');
        // Add the spinning class to the text element
        document.querySelector('.spinning-text').classList.add('spin');
    } else {
        // Pause the audio
        audio.pause();
        // Update the play/pause button icon
        playPauseButton.querySelector('.play-pause-icon').classList.remove('fa-pause');
        playPauseButton.querySelector('.play-pause-icon').classList.add('fa-play');
        // Remove the spinning class from the text element
        document.querySelector('.spinning-text').classList.remove('spin');
    }
});

// Add an event listener to the audio element
audio.addEventListener('timeupdate', () => {
    // Get the current time and total time
    const currentTime = audio.currentTime;
    const totalTime = audio.duration;

    // Format the current time and total time
    const formattedCurrentTime = formatTime(currentTime);
    const formattedTotalTime = formatTime(totalTime);

    // Update the current time and total time displays
    document.getElementById('currentTime').textContent = formattedCurrentTime;
    document.getElementById('totalTime').textContent = formattedTotalTime;
});

// Function to format the time
function formatTime(time) {
    const minutes = Math.floor(time / 60);
    const seconds = Math.floor(time % 60);

    return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
}

// Add an error listener to the audio element
audio.addEventListener('error', () => {
    console.error('An error occurred during audio playback.');
});
