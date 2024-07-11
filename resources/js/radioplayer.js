const playPauseButton = document.getElementById('playPauseButton');
const audio = document.getElementById('audio');
const playPauseIcon = playPauseButton.querySelector('.play-pause-icon');
const textContainer = document.getElementById('textContainer');
const currentTimeElement = document.getElementById('currentTime');
const totalTimeElement = document.getElementById('totalTime');

playPauseButton.addEventListener('click', () => {
    console.log('Play/Pause button clicked');
    if (audio.paused) {
        console.log('Playing audio');
        audio.play().catch(error => {
            console.error('Error playing audio:', error);
        });
        playPauseIcon.classList.remove('fa-play');
        playPauseIcon.classList.add('fa-pause');
        textContainer.classList.add('spin');
    } else {
        console.log('Pausing audio');
        audio.pause();
        playPauseIcon.classList.remove('fa-pause');
        playPauseIcon.classList.add('fa-play');
        textContainer.classList.remove('spin');
    }
});

audio.addEventListener('ended', () => {
    console.log('Audio ended');
    playPauseIcon.classList.remove('fa-pause');
    playPauseIcon.classList.add('fa-play');
    textContainer.classList.remove('spin');
});

audio.addEventListener('timeupdate', () => {
    currentTimeElement.textContent = formatTime(audio.currentTime);
});

audio.addEventListener('loadedmetadata', () => {
    totalTimeElement.textContent = formatTime(audio.duration);
});

function formatTime(seconds) {
    const minutes = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
}
