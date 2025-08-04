# Self-hosting AppFlowy with AppFlowy Cloud

AppFlowy is a privacy-first, open-source workspace designed for notes, wikis, projects, and more, giving you full control over your data and customizations. Over the past year, our community-driven approach has focused on delivering a data-privacy-centric, reliable native experience, and extensible platform.

We're thrilled to introduce self-hosting capabilities for AppFlowy, further empowering users to tailor workspaces to their needs. This guide is divided into two key sections:

1. Setting up AppFlowy Cloud on your server.
2. Building AppFlowy with a self-hosted server.

🙏If you have any questions, don't hesitate to contact us on [Discord](https://discord.gg/9Q2xaN37tV).\\

If you prefer a video tutorial, check out this video which demonstrates self-hosting Appflowy with AppFlowy Cloud on EC2

{% embed url="https://youtu.be/PiNKI_X_7Eo" %}
Video tutorial for self-hosting appflowy with appflowy cloud on amazon ec2 instance.
{% endembed %}

## Step 1: Setting Up AppFlowy Cloud

To self-host AppFlowy Cloud, please refer to our comprehensive [deployment guide](https://github.com/AppFlowy-IO/AppFlowy-Cloud/blob/main/doc/DEPLOYMENT.md).

## Step 2: Download the AppFlowy Application

To get started with the AppFlowy application, follow these steps:

1. **Download the Release Package**:
   * Access the latest release package from [AppFlowy Releases](https://github.com/AppFlowy-IO/AppFlowy/releases).
2. **Launch and Configure the Application**:
   * After downloading, open the AppFlowy application.
   *   Click on 'Quick Start'.

       ![Quick Start](https://1003959744-files.gitbook.io/~/files/v0/b/gitbook-x-prod.appspot.com/o/spaces%2Fvs4LQcuzr0JR34ApS5sM%2Fuploads%2Fgit-blob-0e18b575fe4429c6108b56586071d49cf8935956%2Fquick_start.png?alt=media)
   * Navigate to the `Settings` page.
   * On the left sidebar, select `Cloud Setting`.
   *   Choose `AppFlowy Cloud` as your cloud provider.

       ![Choose AppFlowy Cloud](https://1003959744-files.gitbook.io/~/files/v0/b/gitbook-x-prod.appspot.com/o/spaces%2Fvs4LQcuzr0JR34ApS5sM%2Fuploads%2Fgit-blob-85d4f1b2f799a8edb21ae1e93284e26c791b03fe%2Fchoose_appflowy_cloud.png?alt=media)
3. **Set Up Your Server**:
   * Enter your server address in the provided field.
   *   Click `Restart` to apply the changes.

       ![AppFlowy Cloud](https://1003959744-files.gitbook.io/~/files/v0/b/gitbook-x-prod.appspot.com/o/spaces%2Fvs4LQcuzr0JR34ApS5sM%2Fuploads%2Fgit-blob-3ab35dd6989c9fdf2ab68f40638a30a9f6850374%2Ffill_appflowy_cloud.png?alt=media)
4. **Login**:
   * Go the `Setting` page
   *   On the left sidebar, select `User`.

       ![login.png](https://1003959744-files.gitbook.io/~/files/v0/b/gitbook-x-prod.appspot.com/o/spaces%2Fvs4LQcuzr0JR34ApS5sM%2Fuploads%2Fgit-blob-152469f425341b3f67d72ec8ee64fe714afc4bb6%2Flogin_page.png?alt=media)

## FAQ

### Login with password

**Prerequisites**

* You need to have a server running AppFlowy Cloud.
* You need to have AppFlowy App installed on your device.

**Steps**

1. Open the web browser on your device.
2. Go to the server address where AppFlowy Cloud is running.
   * If your host is `myhost.com`, enter `myhost.com` in the browser.
3. Enter your username and password, and click `Sign In`.
4. After signing in, click on the `Open AppFlowy` icon to open the App. ![AppFlowy Cloud Admin Open AppFlowy](https://1003959744-files.gitbook.io/~/files/v0/b/gitbook-x-prod.appspot.com/o/spaces%2Fvs4LQcuzr0JR34ApS5sM%2Fuploads%2Fgit-blob-5ae5e46c27d2cc24caa4c0bff81bd9188e023b1e%2Fopen_appflowy.png?alt=media)

### Admin Panel

1. Same as "Login with password" steps 1-3.
2. Click on `Admin` in the top right corner.
3. You may access admin functionalities in the left sidebar.

## Current Limitations

**Bundle ID Conflict**

The AppFlowy App built with AppFlowy Cloud currently shares the same bundle ID as the official AppFlowy App. Consequently, you cannot install both versions on the same device simultaneously. We are working on a solution for this issue.

**Ongoing Development of Stable Branch**

The process of building AppFlowy with AppFlowy Cloud using the stable branch is ongoing. Rapid developments and frequent updates in [AppFlowy Cloud](https://github.com/AppFlowy-IO/AppFlowy-Cloud) mean that new features are constantly being added.

**Collaborative Editing**

Currently, collaborative editing is not yet fully supported in the AppFlowy App when built with AppFlowy Cloud. Although collaborative editing data is synchronized across different devices via the AppFlowy Cloud server, the user interface does not instantly update with these synced changes. To see the latest data, you need to reopen the document or database. This feature is still in development.
