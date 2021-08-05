# TTS Bot
Text to speech Discord Bot using the Google TTS API and discord.py!

# Setup Guide
The following instructions only apply to the premium edition of the bot which uses the official Google TTS API. The instructions below were written with Ubuntu in mind. You may need to find the equivalent for your respective OS.

# Easy (Public Premium Bot)
Contact Gnome!#6669 for instructions to purchase or join this [Discord](https://discord.gg/zWPWwQC).

# Normal (via Docker)
This method is recommended as Docker simplies the process drastically without any loss.

### Installing Docker

Install Docker by following instructions for your respective system [here](https://docs.docker.com/engine/install/).

NOTE: If you are not on the latest version of your OS you may recieve a download failure error. This is caused by Docker trying to install the highest version possible which may end up being the version for a newer/latest version leading to crashes or download failures.

Instructions to install a specific version are listed in the Docker documentation [here](https://docs.docker.com/engine/install/ubuntu/) in Step 2 under "Install Docker Engine". For those using other OS' follow the instructions "To install a specific version of Docker Engine" in your respective OS' Docker installation instructions.

### Installing Docker-Compose

Install Docker-Compose by following instructions for your respective system [here](https://docs.docker.com/compose/install/).

LINUX NOTE: If `docker-compose` fails upon installation, run `sudo ln -s /usr/local/bin/docker-compose /usr/bin/docker-compose` to create a symbolic link to `/usr/bin`.

### Installing Git

- Run `sudo apt update` to refresh and update your packages.
- Then run `sudo apt install git` to install Git.

### Cloning the Bot Files

Now we want to clone the bot files from GitHub onto the machine we are hosting on.

Before we clone, we want to ensure that we are in the folder where we want our bot files to be. In this example, we will be using the "home" folder. If you are not already in the root directory, simply type `cd` to return to the root directory. Next, run `cd /home` to be in the home directory.

Finally, we will clone the GitHub repository by running `git clone -b premium https://github.com/uwuscutely/Discord-TTS-Bot.git`.

### Obtaining your Google Application Credentials

1) Log in using your Google account in the [Google Cloud Console](https://console.cloud.google.com/home/dashboard).
2) You may create a new project [here](https://console.cloud.google.com/projectcreate) or simply use an existing project. Feel free to set the name of your new or existing project to whatever you wish.
3) Navigate to the Google API Library and locate the "Cloud Text-to-Speech API" or simply click this [link](https://console.cloud.google.com/apis/library/texttospeech.googleapis.com) and click "Enable".
4) Next, naviate to the APIs & Services menu on the homepage and click "Credentials" from the subcategories. At the top, click "Create Credentials" and select "Service account" from the dropdown menu. Enter any name you wish and click "Create and Continue", you may leave the description empty. Then, click "Done" at the bottom.
5) Click on your service account from the credentials menu and click "Keys" at the top. Following this, click "Add Key" and create a new key, select JSON as the key type.
6) A JSON file will now be downloaded to your computer. Rename this file to "gac.json" or just "gac" if you do not have ["File name extensions"](https://fileinfo.com/help/windows_10_show_file_extensions) enabled in Windows File Explorer.

### Creating and Inviting Your Bot

Head over to https://discord.com/developers/applications and create a new application. Next, click "Bot" from the menu on the left and add a bot. Under the "Privileged Gateway Intents" section, enable "Server Memebers Intent". Here you can also change your bot's username and profile picture. 

Next, we will invite the bot to the main Discord server you would like to operate the bot out of. Click OAuth2 on the left. Under scopes, select "bot" and under permissions select "Administrator". Now, copy and paste the generated link into a new tab on your browser. Select the server you would like to invite the bot to and authorize.

### Uploading your Google Application Credentials

Use an FTP software such as [FileZilla](https://filezilla-project.org/) or [WinSCP](https://winscp.net/eng/index.php) and connect to your host. Navigate to the bot's directory and upload the "gac.json" file inside the folder containing all the bot files. Don't close your connection yet as we'll need it in the next step

### Configs

Now that most of the bot files are in place and dependencies are installed. We can start filling out the config file.

Locate `config-docker.ini` within your bot files and rename it to `config.ini`. Then fill it out using the instructions in each section of the config. Remember to save the file.

Next rename `docker-compose-example-premium.yml` to `docker-compose.yml`

### Starting the Bot

- Build the Docker containers with `docker-compose build`
- Run the Docker containers with `docker-compose up` (add `-d` to run in background)
- Now the bot is running in the container and you can use it!

# Hard (Self Host)
Instructions to self host the bot without the use of Docker can be found on the master branch.
