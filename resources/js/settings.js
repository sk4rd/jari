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
