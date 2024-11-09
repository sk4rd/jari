// Get the play/pause button and the audio element
const buttonPlay = document.getElementById("buttonPlay");
const audio = document.getElementById("audio");

if (localStorage.getItem("JWT")) {
  document.getElementById("navbutton").onclick = () =>
    (window.location.href = window.location.href + "/edit");
  document.getElementById("navbutton").innerText = "Edit Radio";
}

// Add an event listener to the play/pause button
buttonPlay.addEventListener("click", () => {
  const icon = buttonPlay.querySelector(".icon");

  // Check if the audio is playing
  if (audio.paused) {
    // Play the audio
    audio.play();
    // Update the play/pause button icon
    icon.classList.remove("fa-play");
    icon.classList.add("fa-pause");
    // Add the spinning class to the text element
    document.querySelector(".curved-text").classList.remove("paused");
  } else {
    // Pause the audio
    audio.pause();
    // Update the play/pause button icon
    icon.classList.remove("fa-pause");
    icon.classList.add("fa-play");
    // Remove the spinning class from the text element
    document.querySelector(".curved-text").classList.add("paused");
  }
});

// Add an error listener to the audio element
audio.addEventListener("error", () => {
  console.error("An error occurred during audio playback.");
});
