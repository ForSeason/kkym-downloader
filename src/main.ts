const { invoke } = window.__TAURI__;

let inputEl: HTMLInputElement | null;
let novelListEl: HTMLDivElement | null;
let rankTimeEl: HTMLCollectionOf<Element> | null;

window.addEventListener("DOMContentLoaded", () => {
    inputEl = document.querySelector("#url-input");
    novelListEl = document.querySelector("#novel-list");
    rankTimeEl = document.getElementsByClassName("rank-time");
});

async function search() {
    if (novelListEl && inputEl) {
        novelListEl.innerHTML = "Requesting list.</br>Please wait...";
        let data = await invoke("search", {
            query: inputEl.value,
        }) as fetch_ranklist_data;
        if (data.status_code == 0) {
            novelListEl.innerHTML = "";
            for (var novel of data.data) {
                let div = document.createElement("div") as HTMLDivElement;
                let btn = document.createElement("button") as HTMLInputElement;
                btn.textContent = novel.author + " / " + novel.name;
                btn.value = JSON.stringify(novel);
                btn.className = 'url-button';
                btn.onclick = () => {
                    download(btn.value);
                };
                div.appendChild(btn);
                novelListEl.appendChild(div);
            }
        } else {
             novelListEl.innerHTML = data.message;
             return;
        }
    }
}

async function fetch_ranklist(input:string) {
    if (novelListEl) {
        novelListEl.innerHTML = "Requesting list.</br>Please wait...";
        let data = await invoke("fetch_ranklist", {
            novelType: input,
            rankTime: get_rank_time(),
        }) as fetch_ranklist_data;
        if (data.status_code == 0) {
            novelListEl.innerHTML = "";
            for (var novel of data.data) {
                let div = document.createElement("div") as HTMLDivElement;
                let btn = document.createElement("button") as HTMLInputElement;
                btn.textContent = novel.author + " / " + novel.name;
                btn.value = JSON.stringify(novel);
                btn.className = 'url-button';
                btn.onclick = () => {
                    download(btn.value);
                };
                div.appendChild(btn);
                novelListEl.appendChild(div);
            }
        } else {
             novelListEl.innerHTML = data.message;
             return;
        }
    }
}

async function download(novel:string) {
    console.log(JSON.parse(novel));
    let res = await invoke("download", { novel: JSON.parse(novel) }) as string;
    if (res != "") {
        alert(res);
    } else {
        alert("download completed.");
    }
}

function get_rank_time() {
    if (rankTimeEl) {
        for (var i = 0; i < rankTimeEl.length; i++) {
            let input = rankTimeEl[i] as HTMLInputElement;
            if (input.checked) {
                return input.value
            }
        }
    }
    return "entire";
}

window.fetch_ranklist = fetch_ranklist;
window.search = search;

type fetch_ranklist_data = {
    status_code: number
    message: string
    data: Array<novel>
}

type novel = {
    name: string
    author: string
    url: string
}
