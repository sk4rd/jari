if (localStorage.getItem("JWT")) {
    document.getElementById("navbutton").onclick = () => window.location.href="/auth/settings";
    document.getElementById("navbutton").innerText = "Settings"
}
function get_search_contents() {
        var radioname = document.getElementById("search-input").value;
}

