# Geohash Prepper

- cd into the project
- notice the Chicago_Speed_Camera_Locations.csv file and notice its layout
- then run simply run `cargo run -- Chicago_Speed_Camera_Locations.csv output` to execute the prepper
   that will read the CSV file and convert the lat/lon into a geohash and store the results in the geodata.csv file 
   or we can also specify the output file name like `cargo run -- Chicago_Speed_Camera_Locations.csv myoutputfile.csv`

- open the geodata.csv file to see the results
- if results are good copy it into the GPS project for the builder to store the data in the memory during compilation


