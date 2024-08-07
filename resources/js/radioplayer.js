document.addEventListener('DOMContentLoaded', () => {
    const playPauseButton = document.getElementById('playPauseButton');
    const audio = document.getElementById('audio');
    const playPauseIcon = playPauseButton.querySelector('.play-pause-icon');
    const textContainer = document.getElementById('textContainer');
    const currentTimeElement = document.getElementById('currentTime');
    const totalTimeElement = document.getElementById('totalTime');

    playPauseButton.addEventListener('click', () => {
        if (audio.paused) {
            audio.play().catch(error => {
                console.error('Error playing audio:', error);
            });
            updateUIForPlaying();
        } else {
            audio.pause();
            updateUIForPaused();
        }
    });

    audio.addEventListener('ended', updateUIForPaused);

    audio.addEventListener('timeupdate', () => {
        currentTimeElement.textContent = formatTime(audio.currentTime);
    });

    audio.addEventListener('loadedmetadata', () => {
        totalTimeElement.textContent = formatTime(audio.duration);
    });

    function updateUIForPlaying() {
        playPauseIcon.classList.remove('fa-play');
        playPauseIcon.classList.add('fa-pause');
        textContainer.classList.add('spin');
    }

    function updateUIForPaused() {
        playPauseIcon.classList.remove('fa-pause');
        playPauseIcon.classList.add('fa-play');
        textContainer.classList.remove('spin');
    }

    function formatTime(seconds) {
        const minutes = Math.floor(seconds / 60);
        const secs = Math.floor(seconds % 60);
        return `${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
    }

    // Ensure total time is displayed if metadata is already loaded
    if (audio.readyState >= 1) {
        totalTimeElement.textContent = formatTime(audio.duration);
    }
});
