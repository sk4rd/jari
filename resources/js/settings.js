// Add JavaScript functionality for settings page

// Update user function
function updateUser() {
    // Send PUT request to /{username} endpoint
    fetch(`/${document.getElementById("username").value}`, {
        method: "PUT",
        headers: {
            "Content-Type": "application/json"
        },
        body: JSON.stringify({
            // Add updated user data here
        })
    })
        .then(response => response.json())
        .then(data => console.log(data))
        .catch(error => console.error(error));
}

// Delete user function
function deleteUser() {
    // Send DELETE request to /auth/user endpoint
    fetch("/auth/user", {
        method: "DELETE"
    })
        .then(response => response.json())
        .then(data => console.log(data))
        .catch(error => console.error(error));
}

// Add radio function
function addRadio() {
    // Send POST request to /radios endpoint
    fetch("/radios", {
        method: "POST",
        headers: {
            "Content-Type": "application/json"
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