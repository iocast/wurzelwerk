+++
title = "Wurzelwerk"
description = "A desktop board for families and clans – draw people, clans and the lines between them."
template = "index.html"

[extra]
tagline = "Every family is a root network."
lead = "Wurzelwerk is a fast, local-first desktop board for families and clans. Place people, gather them into clans and draw the lines between them – the board keeps it tidy, even with ten thousand people on it."

[[extra.features]]
icon = "board"
title = "An infinite board"
text = "Pan and zoom freely. People are cards with an accent bar in their clan's colour; zoom out and they turn into coloured blocks and finally a density map."

[[extra.features]]
icon = "clans"
title = "Clans as hulls"
text = "Clans wrap their members in a coloured hull. A person can belong to several clans – the hulls simply overlap."

[[extra.features]]
icon = "lines"
title = "Lines that route themselves"
text = "Name a line and it picks its style: married is plain, parents of is an arrow, godfather of is dashed. Routes are orthogonal, avoid cards and stay out of each other's way."

[[extra.features]]
icon = "layout"
title = "One-click auto-layout"
text = "Partners side by side, children in the next generation, as few crossings as possible and clan groups packed into a compact board."

[[extra.features]]
icon = "files"
title = "Notes & files"
text = "Every person and clan has Markdown notes. Drop documents and photos on a person – they are copied into the workspace next to the database."

[[extra.features]]
icon = "local"
title = "Yours, on your disk"
text = "A workspace is just a folder with an embedded database. No account, no cloud. English and German UI, light and dark appearance."

[[extra.internals]]
value = "10 000"
label = "people on one board, still smooth"
text = "GPU-cached static layer, spatial-grid culling, batched paths and level of detail."

[[extra.internals]]
value = "A*"
label = "orthogonal line router"
text = "Searches the sparse visibility grid of the cards with a bend penalty, then nudges parallel segments apart."

[[extra.internals]]
value = "Sugiyama"
label = "layered auto-layout"
text = "Barycentre sweeps with exact crossing counts and quadratic coordinate assignment per row."

[[extra.internals]]
value = "0 ms"
label = "UI time spent saving"
text = "Every write goes to an embedded RedDB on a worker thread, checkpointed when the queue drains."

[[extra.shortcuts]]
keys = ["V"]
label = "Select"
[[extra.shortcuts]]
keys = ["G"]
label = "Clan tool"
[[extra.shortcuts]]
keys = ["P"]
label = "Person tool"
[[extra.shortcuts]]
keys = ["C"]
label = "Connect"
[[extra.shortcuts]]
keys = ["F"]
label = "Fit board"
[[extra.shortcuts]]
keys = ["Ctrl", "F"]
label = "Search"
[[extra.shortcuts]]
keys = ["Ctrl", "B"]
label = "Sidebar"
[[extra.shortcuts]]
keys = ["Del"]
label = "Delete selection"

[[extra.downloads]]
os = "Windows"
files = ".msi · .exe"
[[extra.downloads]]
os = "macOS"
files = ".dmg"
[[extra.downloads]]
os = "Linux"
files = ".deb · .AppImage"
+++
