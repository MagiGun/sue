package com.magigun.sue;

import android.app.NativeActivity;
import android.app.PendingIntent;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.ContentResolver;
import android.content.Intent;
import android.content.pm.PackageInstaller;
import android.database.Cursor;
import android.net.Uri;
import android.os.Build;
import android.provider.DocumentsContract;
import android.provider.OpenableColumns;
import android.webkit.MimeTypeMap;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * Sue's activity: Android's native activity, which runs Sue from
 * libsue_android.so, plus the system's own file pickers (the Storage
 * Access Framework). Sue calls these methods through JNI
 * (crates/sue-platform/src/android.rs); there are no native methods.
 * The picker answers later: Sue polls takePicked, and copies documents in
 * and out on its own worker threads with copyIn, copyOut, createIn and
 * listIn. It also opens web pages (openUrl), installs Sue's own
 * updates (installPackage) and reaches the clipboard (copyText,
 * pasteText).
 */
public class SueActivity extends NativeActivity {
    private static final int PICK = 0x5E1;
    private static final String INSTALLED = "com.magigun.sue.INSTALLED";

    /** The last picker's answer, until taken: null while it is open. */
    private String picked;

    /**
     * Shows the system's picker: 0 a document to open, 1 where to save
     * fileName, 2 a folder. Extensions narrow what it offers.
     */
    public void showPicker(int kind, String[] extensions, String fileName) {
        final Intent intent;
        int grants = Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION;
        if (kind == 2) {
            intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
        } else if (kind == 1) {
            intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType(mimeType(fileName));
            intent.putExtra(Intent.EXTRA_TITLE, fileName);
        } else {
            intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            String[] types = mimeTypes(extensions);
            intent.setType(types.length == 1 ? types[0] : "*/*");
            if (types.length > 1) {
                intent.putExtra(Intent.EXTRA_MIME_TYPES, types);
            }
        }
        intent.addFlags(grants);
        synchronized (this) {
            picked = null;
        }
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    startActivityForResult(intent, PICK);
                } catch (Exception error) {
                    answer("!" + why(error));
                }
            }
        });
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        if (request != PICK) {
            super.onActivityResult(request, result, data);
            return;
        }
        Uri uri = result == RESULT_OK && data != null ? data.getData() : null;
        answer(uri == null ? "" : uri.toString());
    }

    private synchronized void answer(String text) {
        picked = text;
    }

    /**
     * The picker's answer, once: null while it is open, "" when it was
     * cancelled, "!" and why when it failed, else the chosen URI.
     */
    public synchronized String takePicked() {
        String text = picked;
        picked = null;
        return text;
    }

    /** The name a document or folder shows people. */
    public String displayName(String text) {
        Uri uri = Uri.parse(text);
        try {
            if (DocumentsContract.isTreeUri(uri) && !DocumentsContract.isDocumentUri(this, uri)) {
                uri = DocumentsContract.buildDocumentUriUsingTree(uri, DocumentsContract.getTreeDocumentId(uri));
            }
            Cursor cursor = getContentResolver().query(
                    uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null);
            if (cursor != null) {
                try {
                    if (cursor.moveToFirst() && cursor.getString(0) != null) {
                        return cursor.getString(0);
                    }
                } finally {
                    cursor.close();
                }
            }
        } catch (Exception error) {
            // The last part of the URI will do.
        }
        String last = uri.getLastPathSegment();
        if (last == null) {
            return "file";
        }
        int colon = last.lastIndexOf(':');
        int slash = last.lastIndexOf('/');
        return last.substring(Math.max(colon, slash) + 1);
    }

    /** Copies the document at uri into the file at path: null when done, else why not. */
    public String copyIn(String uri, String path) {
        try (InputStream in = getContentResolver().openInputStream(Uri.parse(uri));
                OutputStream out = new FileOutputStream(path)) {
            if (in == null) {
                return "the document cannot be read";
            }
            pump(in, out);
            return null;
        } catch (Exception error) {
            return why(error);
        }
    }

    /** Writes the file at path over the document at uri: null when done, else why not. */
    public String copyOut(String path, String uri) {
        ContentResolver resolver = getContentResolver();
        Uri document = Uri.parse(uri);
        OutputStream out;
        try {
            out = resolver.openOutputStream(document, "wt");
        } catch (Exception truncateNotOffered) {
            try {
                out = resolver.openOutputStream(document, "w");
            } catch (Exception error) {
                return why(error);
            }
        }
        if (out == null) {
            return "the document cannot be written";
        }
        try (InputStream in = new FileInputStream(path); OutputStream sink = out) {
            pump(in, sink);
            return null;
        } catch (Exception error) {
            return why(error);
        }
    }

    /** Makes a document called name in the chosen folder: its URI, or "!" and why not. */
    public String createIn(String tree, String name) {
        try {
            Uri root = Uri.parse(tree);
            Uri folder = DocumentsContract.buildDocumentUriUsingTree(root, DocumentsContract.getTreeDocumentId(root));
            Uri made = DocumentsContract.createDocument(getContentResolver(), folder, mimeType(name), name);
            return made == null ? "!the folder made no file" : made.toString();
        } catch (Exception error) {
            return "!" + why(error);
        }
    }

    /**
     * The documents right inside the chosen folder, folders left out: a
     * line each, its URI, a tab and its name; or "!" and why not.
     */
    public String listIn(String tree) {
        try {
            Uri root = Uri.parse(tree);
            Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(root, DocumentsContract.getTreeDocumentId(root));
            Cursor cursor = getContentResolver().query(children, new String[] {
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE,
            }, null, null, null);
            if (cursor == null) {
                return "!the folder cannot be read";
            }
            StringBuilder list = new StringBuilder();
            try {
                while (cursor.moveToNext()) {
                    String name = cursor.getString(1);
                    if (name == null || DocumentsContract.Document.MIME_TYPE_DIR.equals(cursor.getString(2))) {
                        continue;
                    }
                    list.append(DocumentsContract.buildDocumentUriUsingTree(root, cursor.getString(0)))
                            .append('\t')
                            .append(name.replace('\n', ' '))
                            .append('\n');
                }
            } finally {
                cursor.close();
            }
            return list.toString();
        } catch (Exception error) {
            return "!" + why(error);
        }
    }

    private static void pump(InputStream in, OutputStream out) throws java.io.IOException {
        byte[] buffer = new byte[1 << 16];
        int read;
        while ((read = in.read(buffer)) != -1) {
            out.write(buffer, 0, read);
        }
        out.flush();
    }

    private static String why(Exception error) {
        String message = error.getMessage();
        return message == null || message.isEmpty() ? error.getClass().getSimpleName() : message;
    }

    /** The MIME type of a file name's extension, or any bytes. */
    private static String mimeType(String name) {
        int dot = name == null ? -1 : name.lastIndexOf('.');
        String type = dot < 0 ? null : MimeTypeMap.getSingleton()
                .getMimeTypeFromExtension(name.substring(dot + 1).toLowerCase(Locale.ROOT));
        return type == null ? "application/octet-stream" : type;
    }

    /**
     * The MIME types of the extensions, or every kind when one has none
     * Android knows (a .sue project), so that no file is hidden.
     */
    private static String[] mimeTypes(String[] extensions) {
        List<String> types = new ArrayList<String>();
        if (extensions != null) {
            for (String extension : extensions) {
                String type = MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension.toLowerCase(Locale.ROOT));
                if (type == null) {
                    return new String[] {"*/*"};
                }
                if (!types.contains(type)) {
                    types.add(type);
                }
            }
        }
        return types.isEmpty() ? new String[] {"*/*"} : types.toArray(new String[0]);
    }

    /** Puts text on the system clipboard: null, or why it failed. */
    public String copyText(String text) {
        try {
            ClipboardManager clipboard =
                (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
            clipboard.setPrimaryClip(ClipData.newPlainText("Sue", text));
            return null;
        } catch (Exception error) {
            return why(error);
        }
    }

    /** The text on the system clipboard, or null when there is none. */
    public String pasteText() {
        try {
            ClipboardManager clipboard =
                (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
            ClipData clip = clipboard.getPrimaryClip();
            if (clip == null || clip.getItemCount() == 0) {
                return null;
            }
            CharSequence text = clip.getItemAt(0).coerceToText(this);
            return text == null ? null : text.toString();
        } catch (Exception error) {
            return null;
        }
    }

    /** Opens a web page in the system's browser: null, or why it failed. */
    public String openUrl(String url) {
        try {
            Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(url));
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            startActivity(intent);
            return null;
        } catch (Exception error) {
            return why(error);
        }
    }

    /**
     * Hands a downloaded APK (Sue's update) to the package installer, which
     * asks the person first: null, or why it failed. The installer's answer
     * comes back to onNewIntent.
     */
    public String installPackage(String path) {
        try {
            PackageInstaller installer = getPackageManager().getPackageInstaller();
            PackageInstaller.SessionParams params =
                new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL);
            int id = installer.createSession(params);
            PackageInstaller.Session session = installer.openSession(id);
            try {
                File file = new File(path);
                InputStream in = new FileInputStream(file);
                OutputStream out = session.openWrite("sue.apk", 0, file.length());
                try {
                    byte[] buffer = new byte[64 * 1024];
                    for (int read; (read = in.read(buffer)) != -1; ) {
                        out.write(buffer, 0, read);
                    }
                    session.fsync(out);
                } finally {
                    in.close();
                    out.close();
                }
                Intent intent = new Intent(this, SueActivity.class).setAction(INSTALLED);
                int flags = PendingIntent.FLAG_UPDATE_CURRENT;
                if (Build.VERSION.SDK_INT >= 31) {
                    flags |= PendingIntent.FLAG_MUTABLE;
                }
                PendingIntent done = PendingIntent.getActivity(this, id, intent, flags);
                session.commit(done.getIntentSender());
            } finally {
                session.close();
            }
            return null;
        } catch (Exception error) {
            return why(error);
        }
    }

    /** The package installer's answer: it may want the person to confirm. */
    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        if (intent == null || !INSTALLED.equals(intent.getAction())) {
            return;
        }
        int status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE);
        if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
            Intent confirm = intent.getParcelableExtra(Intent.EXTRA_INTENT);
            if (confirm != null) {
                startActivity(confirm);
            }
        }
    }
}
