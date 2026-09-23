CREATE TABLE people (
  id bigint unsigned NOT NULL AUTO_INCREMENT PRIMARY KEY,
  name varchar(255) NOT NULL,
  age int,
  flag tinyint(1) NOT NULL DEFAULT 0,
  tiny tinyint,
  price decimal(10,2),
  ratio double NOT NULL,
  born date,
  at datetime(6),
  ts timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP,
  t time,
  y year,
  data json,
  raw blob,
  bits bit(8),
  mood enum('sad','ok','happy') NOT NULL,
  opts set('a','b'),
  big bigint
);
CREATE TABLE pets (id int NOT NULL AUTO_INCREMENT PRIMARY KEY, owner_id bigint unsigned NOT NULL, name text NOT NULL);
