function handleLogin(event) {
    event.preventDefault(); // Prevents from sending the form
    window.location.href = 'start.html'; // Redirect to Startpage
}

function handleCredentialResponse(response) {
    console.log(response);
    // Handle the credential response
}