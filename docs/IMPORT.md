# Importing browser data

Open NoRAM Settings → Import browser data. Choose a detected profile (or its profile folder), select data, and preview. Nothing is copied until you click **Import selected data**. Bookmarks are deduplicated by URL; history retains up to 2,000 entries. Import does not modify the source profile. Private windows cannot import.

Brave, Chrome, Edge, Chromium, Vivaldi and Opera support bookmarks and history. Firefox supports bookmarks, history and ordinary cookies; container and partitioned cookies are skipped. If a database is locked, close the source browser and retry. A custom profile folder works for portable profiles too. Settings, passwords, payments, autofill and extensions are not transferred.

Bookmark files: Netscape HTML or Chromium Bookmarks JSON. Cookie files: JSON cookie arrays (or an object with a `cookies` array), or Netscape cookies.txt including HttpOnly entries. Limits: 16 MB per file and 5,000 cookies. Expired, malformed and partitioned cookies are skipped. Matching destination cookies are replaced by name/domain/path. Import can transfer login sessions, but sites may require a fresh sign-in.

## Chromium cookie export helper

Modern Chromium cookie databases are encrypted and cannot be copied directly. The included helper uses the source browser's supported cookie API, only when you explicitly click Export.

1. In Brave/Chrome/Edge open its extension manager, enable Developer mode, choose **Load unpacked**, and select this project's `tools/browser-cookie-export` directory.
2. Open the helper's toolbar action. Optionally restrict export to one domain. Click **Export local JSON file**.
3. In NoRAM Settings → Import browser data → Choose cookie file, select the JSON export. Review the counts and click Import selected data.
4. Delete the export file and remove the helper when finished. Treat the export as sensitive as a password: it can contain active login sessions.

The helper has cookie and host permissions so it can export ordinary cookies, including HttpOnly cookies. It has no remote scripts, analytics or network requests. It does not decrypt databases or access private cookie stores. NoRAM applies cookies to the website profile, never the privileged Settings UI. Cookie values are excluded from previews and diagnostic messages.
