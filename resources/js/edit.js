
let file_upload = document.getElementById("upload");
file_upload.addEventListener("change", () => {
    let file = file_upload.files[0];
    let rx = /([^\/]*)\/edit/g;
    let id = rx.exec(document.URL)[1];
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

