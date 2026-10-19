# English (fallback) user interface text.

## toolbar

tool-select = Select
tool-connect = Connect
tool-auto-layout = Auto-layout
tool-fit-to-window = Fit to window
tool-settings = Settings
tool-fit = Fit
tool-fit-to-window-tooltip = Fit to window (F)
tool-auto-layout-tooltip = Arrange everyone by generation and clan
tool-hide-sidebar = Hide sidebar
tool-show-sidebar = Show sidebar
tool-switch-workspaces = Switch or create workspaces
tool-search-placeholder = Search people…  Ctrl+F
tool-light-mode = Light mode
tool-dark-mode = Dark mode
tool-timeline = Timeline
tool-timeline-tooltip = Show the board as it was in a chosen year

## board overlays

zoom-in-tooltip = Zoom in (+)
zoom-out-tooltip = Zoom out (−)
zoom-reset-tooltip = Reset to 100% (0)
hint-add-person = Click anywhere on the board to place a person
hint-connect = Drag from one person onto another to connect them
hint-pick-clan = Pick a clan first
hint-members = Click people to add them to or remove them from { $clan }
hint-add-person-in-clan = Click anywhere on the board to place a person in { $clan }
hint-add-clan = Click on the board to place a new clan
hint-name-line = Name the new line in the panel on the right
empty-board-title = Start your family board
empty-board-body = Add the first person, then connect relatives and group them into clans.
empty-board-add-person = Add person
timeline-play = Play
timeline-pause = Pause
timeline-close = Close timeline
timeline-living = { $count } living

## sidebar

sidebar-clans = CLANS
sidebar-people = PEOPLE
sidebar-books = BOOKS
sidebar-more-use-search = … { $count } more - use search
list-more = … { $count } more
sidebar-add-clan = Add clan
sidebar-empty-clan = empty clan
sidebar-deselect-clan = Deselect clan – new people join no clan
sidebar-new-clan = New clan
sidebar-new-book = New book
sidebar-new-page = New page
sidebar-no-clans = No clans yet.
sidebar-no-people = No people yet.
sidebar-no-match = Nobody matches your search.

## inspector - empty

inspector-nothing-selected = Nothing selected
inspector-nothing-selected-body = Select a person, line or clan to see and edit its details here.
inspector-getting-started = GETTING STARTED
help-add-people = Add people
help-add-people-body = + Person: click on the board to place someone.
help-connect = Connect relatives
help-connect-body = Connect: drag from a person (or a handle of the selected person) onto another person, then name the line.
help-clans = Group into clans
help-clans-body = Clan: click on the board to place a clan, then use + Person inside its box. New people join the clan selected on the left; click it again for no clan.
help-lines = Shape the lines
help-lines-body = Select: drag a line label to move it, or drag a line segment to add/move a bend.
inspector-keyboard = KEYBOARD
key-zoom = Zoom in / out
key-search = Search people
key-delete = Delete selection
key-cancel = Cancel / deselect
key-wheel = Wheel
key-wheel-zoom = Zoom
key-drag = Drag empty space
key-drag-pan = Pan

## inspector - line

line-name-title = NAME THE LINE
line-relationship = RELATIONSHIP
line-name-placeholder = e.g. godfather of
line-common = Or pick a common one:
line-create = Create line
cancel = Cancel
line-selected = SELECTED LINE
line-missing-person = Missing person
line-route-help = Drag a label to move it or drag a segment to reshape the route.
line-corner-selected = Selected corner: press Delete or remove it below.
line-remove-corner = Remove selected corner
line-corner-help = Click a route corner to select and remove it.
line-reset-route = Reset to automatic route
line-route = ROUTE
line-delete-help = Press Delete to remove this line.
line-remove = Remove line

## line presets

link-married = married
link-parents-of = parents of
link-father-of = father of
link-mother-of = mother of
link-siblings = siblings
link-partner = partner
link-godfather-of = godfather of
link-godmother-of = godmother of

## inspector - person

person-details = DETAILS
person-name = Name
person-born = Born
person-died = Died
person-born-placeholder = e.g. 1921
person-died-placeholder = e.g. 1999
person-dates-unknown = Dates unknown
person-add-clan = + clan
person-remove-from-clan = Remove from clan
person-no-clans = Create a clan in the sidebar first.
person-lines = LINES · { $count }
person-edit-line = Edit line
person-lines-help = Drag a handle onto another person to add a line.
person-lines-route-help = On the board, drag a line label to reposition it or drag a segment to change the route.
person-files = FILES · { $count }
person-delete-file = Delete file
person-drop-files = Drop files here or choose…
person-attach-files = Attach files
person-open-notes = Open notes
person-write-notes = Write notes
person-notes-hint = Markdown, saved automatically
person-delete-help = Removes the person with their lines, notes and files.
person-delete = Delete person

## inspector - clan

clan = Clan
clan-name = Clan name
clan-member-count = { $count ->
        [one] { $count } member
       *[other] { $count } members
    }
clan-colour = COLOUR
clan-edit-members = Add / remove members
clan-done-adding = Done adding
clan-members = MEMBERS · { $count }
clan-no-members = No members yet.
clan-fit = Fit to clan
clan-members-help = Click “Add / remove members”, then click people on the board. A person can be in several clans.
clan-delete-help = Members stay on the board; only the clan is removed.
clan-delete = Delete clan

## notes

notes-back = Back to board
notes-book-name = Book name
notes-page-name = Page name
notes-delete-page = Delete page
notes-title = Notes · { $title }
notes-saving = Saving…
notes-saved = Saved

## defaults and notices

default-person-name = New person
default-person = Person
default-clan-name = Clan { $number }
notice-select-clan = Select a clan in the sidebar, then click people to add/remove them.
notice-select-person = Select a person first.
notice-drop-files = Select a person, then drop files to attach them.
notice-no-dates = Add birth or death years to people to use the timeline.

## confirmation

confirm-delete-title = Delete { $name }?
confirm-delete = Delete
confirm-delete-person = Their lines, notes and attached files are removed as well. This cannot be undone.
confirm-delete-clan = The members stay on the board. The clan and its notes are removed. This cannot be undone.
confirm-delete-page = The page is removed from the book. This cannot be undone.

## status bar

status-counts = { $people ->
        [one] { $people } person
       *[other] { $people } people
    } · { $clans ->
        [one] { $clans } clan
       *[other] { $clans } clans
    } · { $lines ->
        [one] { $lines } line
       *[other] { $lines } lines
    }
status-saved = all changes saved
status-saving = saving… ({ $pending })
status-save-error = save error: { $error }

## welcome

welcome-title = Welcome to Wurzelwerk
welcome-body = Create a workspace to get started. A workspace is a folder that holds your families, clans, notes and files.
welcome-location = WORKSPACE LOCATION
welcome-location-help = Pick an existing workspace folder to open it, or a new folder to start fresh.
welcome-recent = RECENT WORKSPACES
welcome-folder-path = Folder path
welcome-create = Create workspace
welcome-choose-folder = Choose workspace folder
welcome-enter-location = Please enter a location.
example-title = EXAMPLE WORKSPACE
example-help = A generated family board (11 clans, about 600 people over four centuries) to explore the board and the timeline. It is created on first use.
example-open = Open example workspace
error-create-folder = cannot create { $path }: { $error }
error-open-database = cannot open the database in { $path }: { $error }

## settings dialog

settings-general = General
settings-workspace = Workspace
settings-updates = Updates
settings-about = About
settings-close = Close
settings-appearance = APPEARANCE
settings-appearance-help = Choose a light or dark look, or follow the system.
settings-system = System
settings-light = Light
settings-dark = Dark
settings-language = LANGUAGE
settings-language-help = Language of the user interface.
settings-current-workspace = CURRENT WORKSPACE
settings-current-workspace-help = Everything you see on the board is stored in this folder.
settings-open-folder = Open folder
settings-relations = RELATIONSHIPS
settings-relations-help = Line names you typed yourself. They are offered again when you draw a line; give each a name per language. Renaming also renames the lines that use it.
settings-no-relations = No own relationships yet. Type a new name when you create a line.
settings-remove-relation = Remove from the suggestions (lines keep their name)
settings-switch-workspace = SWITCH WORKSPACE
settings-switch-workspace-help = Pick a folder to open it as workspace. A new or empty folder becomes a new workspace.
settings-browse = Browse…
settings-open = Open
settings-no-recent = No other workspaces yet.
settings-recent-help = Open one of the workspaces you used before.
settings-version = VERSION
settings-current-version = Current version: { $version }
settings-check-now = Check now
settings-checking = Checking…
settings-up-to-date = You are up to date.
settings-automatic-check = AUTOMATIC CHECK
settings-automatic-check-help = Looks for a new release when Wurzelwerk starts and downloads it in the background.
settings-check-on-start = Check for updates on start
about-version = Version { $version }
about-tagline = Clans & families board
about-settings-file = SETTINGS FILE
about-settings-file-help = Global settings are stored here:
about-licence = LICENCE
about-source-code = SOURCE CODE
about-open-github = Open on GitHub
error-save-settings = Could not save settings: { $error }

## updates

update-available = Version { $version } is available.
update-ready = Version { $version } is ready to install.
update-downloading = Downloading version { $version }…
update-download = Download
update-dismiss = Dismiss
update-restart = Restart now
update-check-failed = Update check failed: { $error }
update-failed = Update failed: { $error }
