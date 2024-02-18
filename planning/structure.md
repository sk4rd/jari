# Structure
How we structure the program.

## Frontend
The frontend needs to provide an interface to users to:
 - Find radios
 - Listen to a radio
 - Configure their own radio (authenticated)
 - [Login](#login)
 - ?Send a voiceover
## Backend
### REST-API
The API allows the frontend to request:
 - A list of radios
 - An audiostream of a radio
 - To change the configuration of a radio (authenticated)
 - Upload a song to a radio (authenticated)
 - Remove a song from a radio (authenticated)
 - ?To [login](#login)
 - ?Enable a voiceover
### Audio-Streaming
This should:
 - Read audio files
 - Do basic mixing (trim, change volume)
 - Send a finished audiostream to listeners
 - Update its configuration live
 - Manage listeners live
 - ?Mix in a voiceover
### Login
This should be able to:
 - Add new users
 - Authenticate existing users

## Connections
 - [Frontend](#frontend) - [REST-API](#rest-api) (main frontend-backend connection)
 - [REST-API](#rest-api) - [Audio-Streaming](#audio-streaming) (changing config)
 - [Audio-Streaming](#audio-streaming) - [Frontend](#frontend) (audiostream)
 - [REST-API](#rest-api) - [Login](#login) (auth)
 - ?[Frontend](#frontend) - [Login](#login) (login)
