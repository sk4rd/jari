function get_edit_content(){
    let title = document.getElementById("title").value;
    console.log(title);
    let description = document.getElementById("description").value;
    console.log(description)
    let rx = /([^\/]*)\/edit/g;
    let id = rx.exec(document.URL)[1];
    fetch("/" + id, {method:"POST", body: JSON.stringify({title, description}), headers: {"Content-Type":"application/json"}})
}
